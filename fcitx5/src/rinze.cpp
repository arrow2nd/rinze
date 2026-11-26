#include <atomic>
#include <cerrno>
#include <cstdlib>
#include <ctime>
#include <fcntl.h>
#include <filesystem>
#include <fstream>
#include <memory>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <unistd.h>
#include <utility>
#include <vector>

#include <fcitx-utils/event.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/log.h>
#include <fcitx-utils/standardpaths.h>
#include <fcitx-utils/trackableobject.h>
#include <fcitx/action.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/event.h>
#include <fcitx/globalconfig.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/menu.h>
#include <fcitx/statusarea.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>
#include <fcitx/userinterfacemanager.h>

#include "rinze_client.h"

namespace {

FCITX_DEFINE_LOG_CATEGORY(rinze_log, "rinze");
#define RINZE_INFO() FCITX_LOGC(rinze_log, Info)
#define RINZE_WARN() FCITX_LOGC(rinze_log, Warn)

using fcitx::CapabilityFlag;
using fcitx::InputContext;
using fcitx::Key;
using fcitx::KeyState;
using fcitx::KeyStates;
using fcitx::Text;
using fcitx::TextFormatFlag;
using fcitx::UserInterfaceComponent;

constexpr uint64_t STARTUP_POLL_USEC = 100'000;
// 直接入力にするアプリの名前 (fcitx5 が知らせるもの) を 1 行に 1 つ書く。fcitx5 の設定のディレクトリ (~/.config/fcitx5) の中
constexpr const char *DIRECT_PROGRAMS = "rinze/direct-programs";

struct ServiceFree {
    void operator()(RinzeService *p) const { rinze_service_free(p); }
};
struct ClientFree {
    void operator()(RinzeClient *p) const { rinze_client_free(p); }
};
struct ResponseFree {
    void operator()(RinzeResponse *p) const { rinze_response_free(p); }
};
using ServicePtr = std::unique_ptr<RinzeService, ServiceFree>;
using ClientPtr = std::unique_ptr<RinzeClient, ClientFree>;
using ResponsePtr = std::unique_ptr<RinzeResponse, ResponseFree>;

struct ModeInfo {
    RinzeMode mode;
    const char *action;
    const char *label;
    const char *description;
};

constexpr ModeInfo MODES[] = {
    {RINZE_MODE_HIRAGANA, "rinze-mode-hiragana", "かな", "ひらがな"},
    {RINZE_MODE_KATAKANA, "rinze-mode-katakana", "カナ", "カタカナ"},
    {RINZE_MODE_HALFWIDTH_KATAKANA, "rinze-mode-halfwidth-katakana", "ｶﾅ",
     "半角カナ"},
    {RINZE_MODE_FULLWIDTH_LATIN, "rinze-mode-fullwidth-latin", "全英",
     "全角英字"},
    {RINZE_MODE_ASCII, "rinze-mode-ascii", "aA", "ASCII"},
};

const ModeInfo *modeInfo(std::optional<RinzeMode> mode) {
    for (const auto &info : MODES) {
        if (mode == info.mode) {
            return &info;
        }
    }
    return nullptr;
}

std::string envOr(const char *name, const char *fallback) {
    const char *value = std::getenv(name);
    return value && *value ? value : fallback;
}

std::string rinzeKey(const Key &key) {
    KeyStates modifiers = key.states() & KeyState::SimpleMask;
    modifiers.unset(KeyState::Shift);
    if (modifiers == KeyState::Ctrl) {
        // fcitx5 の KeyEvent は Control+g を Control+G に正規化して渡す
        switch (key.sym()) {
        case FcitxKey_g:
        case FcitxKey_G:
            return "ctrl-g";
        case FcitxKey_j:
        case FcitxKey_J:
            return "ctrl-j";
        case FcitxKey_q:
        case FcitxKey_Q:
            return "ctrl-q";
        default:
            return {};
        }
    }
    if (modifiers) {
        return {};
    }
    switch (key.sym()) {
    case FcitxKey_space:
        return "space";
    case FcitxKey_Return:
    case FcitxKey_KP_Enter:
        return "enter";
    case FcitxKey_BackSpace:
        return "backspace";
    default:
        break;
    }
    auto c = Key::keySymToUnicode(key.sym());
    if (c >= 0x21 && c <= 0x7e) {
        return std::string(1, static_cast<char>(c));
    }
    return {};
}

const char *leftContextName(RinzeLeftContext origin) {
    switch (origin) {
    case RINZE_LEFT_CONTEXT_SURROUNDING:
        return "surrounding";
    case RINZE_LEFT_CONTEXT_PREEDIT_REMOVED:
        return "未確定文字列を除いた";
    case RINZE_LEFT_CONTEXT_UNKNOWN:
        break;
    }
    return "なし";
}

class RinzeEngine;

// 入力欄 1 つぶんの rinzed の接続。表示は最後の応答をそのまま写し、入力の状態は rinzed だけが持つ。
class RinzeState final : public fcitx::InputContextProperty,
                         public fcitx::TrackableObject<RinzeState> {
public:
    RinzeState(RinzeEngine &engine, InputContext &ic);

    bool press(std::string key);
    void flush();
    void discard();
    void switchMode(RinzeMode mode);
    void connect();
    void startupFinished(RinzeStatus status, const char *message);
    void restoreNotice();

    std::optional<RinzeMode> mode() const;
    bool passthrough() const;
    bool idle() const;
    bool showsCandidates() const;

private:
    bool apply(RinzeResponse *raw);
    void render();

    RinzeEngine &engine_;
    InputContext &ic_;
    ClientPtr client_;
    ResponsePtr shown_;
    std::string notice_;
    std::string diagnosis_;
};

class RinzeCandidate final : public fcitx::CandidateWord {
public:
    RinzeCandidate(fcitx::TrackableObjectReference<RinzeState> state,
                   std::string key, std::string word, std::string annotation)
        : CandidateWord(Text(std::move(word))), state_(std::move(state)),
          key_(std::move(key)) {
        if (!annotation.empty()) {
            setComment(Text(std::move(annotation)));
        }
    }

    void select(InputContext *) const override {
        if (auto *state = state_.get()) {
            state->press(key_);
        }
    }

private:
    fcitx::TrackableObjectReference<RinzeState> state_;
    std::string key_;
};

class RinzePage final : public fcitx::CandidateList,
                        public fcitx::PageableCandidateList {
public:
    RinzePage(RinzeState &state, const RinzeResponse *response)
        : state_(state.watch()), page_(rinze_response_page(response)),
          pages_(rinze_response_pages(response)) {
        setPageable(this);
        for (size_t i = 0; i < rinze_response_candidate_count(response); i++) {
            std::string key = rinze_response_candidate_label(response, i);
            labels_.emplace_back(key + ":");
            words_.push_back(std::make_unique<RinzeCandidate>(
                state_, key, rinze_response_candidate_word(response, i),
                rinze_response_candidate_annotation(response, i)));
        }
    }

    const Text &label(int idx) const override { return labels_.at(idx); }
    const fcitx::CandidateWord &candidate(int idx) const override {
        return *words_.at(idx);
    }
    int size() const override { return static_cast<int>(words_.size()); }
    int cursorIndex() const override { return -1; }
    fcitx::CandidateLayoutHint layoutHint() const override {
        return fcitx::CandidateLayoutHint::NotSet;
    }

    // x は最初のページでも、その場に 1 件ずつ出す表示に戻る
    bool hasPrev() const override { return true; }
    bool hasNext() const override { return page_ + 1 < pages_; }
    void prev() override {
        if (auto *state = state_.get()) {
            state->press("x");
        }
    }
    void next() override {
        if (auto *state = state_.get()) {
            state->press("space");
        }
    }
    bool usedNextBefore() const override { return false; }
    int totalPages() const override { return static_cast<int>(pages_); }
    int currentPage() const override { return static_cast<int>(page_); }

private:
    fcitx::TrackableObjectReference<RinzeState> state_;
    size_t page_;
    size_t pages_;
    std::vector<Text> labels_;
    std::vector<std::unique_ptr<RinzeCandidate>> words_;
};

class RinzeModeAction final : public fcitx::Action {
public:
    explicit RinzeModeAction(RinzeEngine &engine) : engine_(engine) {}
    std::string shortText(InputContext *ic) const override;
    std::string longText(InputContext *ic) const override;
    std::string icon(InputContext *) const override { return {}; }

private:
    RinzeEngine &engine_;
};

class RinzeModeChoice final : public fcitx::SimpleAction {
public:
    RinzeModeChoice(RinzeEngine &engine, const ModeInfo &info)
        : engine_(engine), mode_(info.mode) {
        setShortText(std::string(info.label) + " - " + info.description);
        setCheckable(true);
    }
    bool isChecked(InputContext *ic) const override;
    void activate(InputContext *ic) override;

private:
    RinzeEngine &engine_;
    RinzeMode mode_;
};

class RinzeDirectChoice final : public fcitx::SimpleAction {
public:
    explicit RinzeDirectChoice(RinzeEngine &engine) : engine_(engine) {
        setShortText("このアプリで直接入力");
        setCheckable(true);
    }
    bool isChecked(InputContext *ic) const override;
    void activate(InputContext *ic) override;

private:
    RinzeEngine &engine_;
};

class RinzeEngine final : public fcitx::InputMethodEngineV2 {
public:
    explicit RinzeEngine(fcitx::Instance *instance)
        : instance_(instance),
          debug_(std::getenv("RINZE_FCITX_DEBUG") != nullptr),
          service_(newService()),
          directProgramsPath_(directProgramsPath()),
          directPrograms_(loadDirectPrograms(directProgramsPath_)),
          factory_([this](InputContext &ic) { return new RinzeState(*this, ic); }),
          modeAction_(*this), directChoice_(*this) {
        auto &ui = instance_->userInterfaceManager();
        ui.registerAction("rinze-mode", &modeAction_);
        modeAction_.setMenu(&modeMenu_);
        for (const auto &info : MODES) {
            choices_.push_back(std::make_unique<RinzeModeChoice>(*this, info));
            ui.registerAction(info.action, choices_.back().get());
            modeMenu_.addAction(choices_.back().get());
        }
        ui.registerAction("rinze-direct", &directChoice_);
        modeMenu_.addAction(&directChoice_);
        instance_->inputContextManager().registerProperty("rinzeState", &factory_);
        // fcitx5 5.1.19 は入力メソッドの名前をパネルの上段に出して知らせを上書きし、1 秒後にパネルを空にする。
        // rinzed が起動に失敗した知らせなどが次の打鍵まで見えなくならないよう、パネルが空になったら出し直す
        panelWatcher_ = instance_->watchEvent(
            fcitx::EventType::InputContextUpdateUI, fcitx::EventWatcherPhase::Default,
            [this](fcitx::Event &event) {
                auto &ui = static_cast<fcitx::InputContextUpdateUIEvent &>(event);
                auto *ic = ui.inputContext();
                if (ui.component() == UserInterfaceComponent::InputPanel &&
                    instance_->inputMethodEngine(ic) == this) {
                    state(ic)->restoreNotice();
                }
            });
    }

    RinzeState *state(InputContext *ic) { return ic->propertyFor(&factory_); }
    RinzeService *service() { return service_.get(); }
    bool debug() const { return debug_; }

    // 直接入力にしたアプリでは、打鍵をすべてアプリに渡し、rinzed に送らない
    bool direct(const InputContext &ic) const { return directPrograms_.contains(ic.program()); }

    // アプリの名前が分からない入力欄や制御文字を含む名前は、ファイル破損を防ぐため切り替えない
    void toggleDirect(InputContext &ic) {
        const auto &program = ic.program();
        if (program.empty()) {
            return;
        }
        for (unsigned char c : program) {
            if (c < 0x20 || c == 0x7f) {
                return;
            }
        }
        if (directPrograms_.erase(program) == 0) {
            state(&ic)->flush();
            directPrograms_.insert(program);
        }
        saveDirectPrograms();
        modeChanged(ic, true);
    }

    // サブモードのラベルと説明。rinzed の応答がまだ無ければ空
    std::pair<std::string, std::string> modeText(InputContext &ic) {
        if (direct(ic)) {
            return {"直接", "直接入力"};
        }
        const auto *info = modeInfo(state(&ic)->mode());
        return info ? std::pair{std::string(info->label), std::string(info->description)}
                    : std::pair<std::string, std::string>{};
    }

    void activate(const fcitx::InputMethodEntry &,
                  fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        ic->statusArea().addAction(fcitx::StatusGroup::InputMethod, &modeAction_);
        if (debug_) {
            RINZE_INFO() << "activate program=" << ic->program()
                         << " frontend=" << ic->frontendName()
                         << " capabilities=" << ic->capabilityFlags().toInteger();
        }
        state(ic)->connect();
    }

    // fcitx5 はフォーカスが外れたとき、ClientUnfocusCommit を持たない入力先には未確定文字列のうち DontCommit でない部分を確定する。
    // rinze は未確定文字列をすべて DontCommit にして確定させず、ここで rinzed の入力途中も捨てる
    void deactivate(const fcitx::InputMethodEntry &,
                    fcitx::InputContextEvent &event) override {
        auto *s = state(event.inputContext());
        if (event.type() == fcitx::EventType::InputContextSwitchInputMethod) {
            s->flush();
        } else {
            s->discard();
        }
    }

    void reset(const fcitx::InputMethodEntry &,
               fcitx::InputContextEvent &event) override {
        state(event.inputContext())->discard();
    }

    void keyEvent(const fcitx::InputMethodEntry &,
                  fcitx::KeyEvent &event) override {
        const auto &key = event.key();
        if (event.isRelease() || key.isModifier()) {
            return;
        }
        // 打鍵を rinzed に送ると読みと語が学習の履歴に残るので、残してはいけない入力欄では送らない
        const auto caps = event.inputContext()->capabilityFlags();
        if (caps.test(CapabilityFlag::Password) || caps.test(CapabilityFlag::Sensitive) ||
            direct(*event.inputContext())) {
            return;
        }
        auto *s = state(event.inputContext());
        auto name = rinzeKey(key);
        if (s->passthrough() && name != "ctrl-j") {
            return;
        }
        if (s->showsCandidates()) {
            const auto &config = instance_->globalConfig();
            if (key.checkKeyList(config.defaultPrevPage())) {
                name = "x";
            } else if (key.checkKeyList(config.defaultNextPage())) {
                name = "space";
            }
        }
        if (name.empty()) {
            if (s->idle()) {
                return;
            }
            if (!key.check(FcitxKey_Escape)) {
                s->flush();
                return;
            }
            name = "ctrl-g";
        }
        if (s->press(name)) {
            event.filterAndAccept();
        }
    }

    std::string subMode(const fcitx::InputMethodEntry &,
                        InputContext &ic) override {
        return modeText(ic).second;
    }

    std::string subModeLabelImpl(const fcitx::InputMethodEntry &,
                                 InputContext &ic) override {
        return modeText(ic).first;
    }

    void modeChanged(InputContext &ic, bool announce) {
        modeAction_.update(&ic);
        ic.updateUserInterface(UserInterfaceComponent::StatusArea);
        const auto &panel = ic.inputPanel();
        if (announce && panel.auxUp().empty() && !panel.candidateList()) {
            instance_->showInputMethodInformation(&ic);
        }
    }

    // rinzed の起動を待つ間、待ち受けを始めたか起動に失敗したかを確かめ、打鍵の無いうちに表示を直す
    void watchStartup(InputContext &ic) {
        waiting_ = ic.watch();
        if (startupTimer_) {
            startupTimer_->setNextInterval(STARTUP_POLL_USEC);
            startupTimer_->setOneShot();
            return;
        }
        startupTimer_ = instance_->eventLoop().addTimeEvent(
            CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + STARTUP_POLL_USEC, 0,
            [this](fcitx::EventSourceTime *source, uint64_t) {
                const char *message = "";
                auto status = rinze_service_poll(service_.get(), &message);
                if (status == RINZE_STATUS_STARTING) {
                    source->setNextInterval(STARTUP_POLL_USEC);
                    source->setOneShot();
                    return true;
                }
                if (auto *ic = waiting_.get()) {
                    state(ic)->startupFinished(status, message);
                }
                return true;
            });
    }

private:
    ServicePtr newService() const {
        auto socket = envOr("RINZE_SOCKET", "");
        auto program = envOr("RINZE_RINZED", "rinzed");
        ServicePtr service(
            rinze_service_new(socket.empty() ? nullptr : socket.c_str(), program.c_str()));
        if (!service) {
            throw std::runtime_error(
                "rinze: 既定のソケットのパスを決められない (RINZE_SOCKET で指定する)");
        }
        return service;
    }

    // 設定のディレクトリが決まらなければ空で、読みも書きもしない
    static std::filesystem::path directProgramsPath() {
        if (const char *path = std::getenv("RINZE_DIRECT_PROGRAMS"); path && *path) {
            return path;
        }
        const auto &config =
            fcitx::StandardPaths::global().userDirectory(fcitx::StandardPathsType::PkgConfig);
        return config.empty() ? config : config / DIRECT_PROGRAMS;
    }

    static std::set<std::string> loadDirectPrograms(const std::filesystem::path &path) {
        std::set<std::string> programs;
        if (path.empty()) {
            return programs;
        }
        std::ifstream in(path);
        for (std::string line; std::getline(in, line);) {
            if (!line.empty()) {
                programs.insert(line);
            }
        }
        return programs;
    }

    // 書いている途中で fcitx5 が落ちても前の内容が残るよう、別のファイルに書いてから置き換える
    void saveDirectPrograms() const {
        if (directProgramsPath_.empty()) {
            RINZE_WARN() << "設定のディレクトリが決まらないので、直接入力にするアプリを覚えられない";
            return;
        }
        std::error_code error;
        std::filesystem::create_directories(directProgramsPath_.parent_path(), error);
        if (error) {
            RINZE_WARN() << "直接入力にするアプリを " << directProgramsPath_
                         << " に書けない: " << error.message();
            return;
        }
        static std::atomic<uint64_t> tempSeq{0};
        std::filesystem::path temporary;
        int fd = -1;
        for (int attempt = 0; attempt < 64; ++attempt) {
            temporary = directProgramsPath_;
            temporary += ".tmp." + std::to_string(getpid()) + "." +
                         std::to_string(tempSeq.fetch_add(1, std::memory_order_relaxed));
            fd = ::open(temporary.c_str(), O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600);
            if (fd >= 0) {
                break;
            }
            if (errno != EEXIST) {
                error = std::error_code(errno, std::generic_category());
                break;
            }
        }
        if (fd < 0) {
            if (!error) {
                error = std::make_error_code(std::errc::file_exists);
            }
            RINZE_WARN() << "直接入力にするアプリを " << directProgramsPath_
                         << " に書けない: " << error.message();
            return;
        }
        std::string content;
        for (const auto &program : directPrograms_) {
            content += program;
            content += '\n';
        }
        size_t written = 0;
        while (written < content.size()) {
            ssize_t n = ::write(fd, content.data() + written, content.size() - written);
            if (n < 0) {
                if (errno == EINTR) {
                    continue;
                }
                error = std::error_code(errno, std::generic_category());
                break;
            }
            written += static_cast<size_t>(n);
        }
        if (::close(fd) < 0 && !error) {
            error = std::error_code(errno, std::generic_category());
        }
        if (!error) {
            std::filesystem::rename(temporary, directProgramsPath_, error);
        }
        if (error) {
            std::error_code ignored;
            std::filesystem::remove(temporary, ignored);
            RINZE_WARN() << "直接入力にするアプリを " << directProgramsPath_
                         << " に書けない: " << error.message();
        }
    }

    fcitx::Instance *instance_;
    bool debug_;
    ServicePtr service_;
    std::filesystem::path directProgramsPath_;
    std::set<std::string> directPrograms_;
    fcitx::FactoryFor<RinzeState> factory_;
    RinzeModeAction modeAction_;
    RinzeDirectChoice directChoice_;
    fcitx::Menu modeMenu_;
    std::vector<std::unique_ptr<RinzeModeChoice>> choices_;
    std::unique_ptr<fcitx::EventSourceTime> startupTimer_;
    fcitx::TrackableObjectReference<InputContext> waiting_;
    std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>> panelWatcher_;
};

std::string RinzeModeAction::shortText(InputContext *ic) const {
    auto [label, description] = engine_.modeText(*ic);
    return label.empty() ? "rinze" : label + " - " + description;
}

std::string RinzeModeAction::longText(InputContext *ic) const {
    return engine_.modeText(*ic).second;
}

bool RinzeDirectChoice::isChecked(InputContext *ic) const { return engine_.direct(*ic); }

void RinzeDirectChoice::activate(InputContext *ic) { engine_.toggleDirect(*ic); }

bool RinzeModeChoice::isChecked(InputContext *ic) const {
    return engine_.state(ic)->mode() == mode_;
}

void RinzeModeChoice::activate(InputContext *ic) {
    engine_.state(ic)->switchMode(mode_);
}

RinzeState::RinzeState(RinzeEngine &engine, InputContext &ic)
    : engine_(engine), ic_(ic), client_(rinze_client_new(engine.service())) {}

std::optional<RinzeMode> RinzeState::mode() const {
    if (!shown_) {
        return std::nullopt;
    }
    return rinze_response_mode(shown_.get());
}

bool RinzeState::passthrough() const {
    return shown_ && rinze_response_passthrough(shown_.get());
}

bool RinzeState::idle() const {
    return !shown_ || rinze_response_idle(shown_.get());
}

bool RinzeState::showsCandidates() const {
    return shown_ && rinze_response_candidate_count(shown_.get()) > 0;
}

bool RinzeState::press(std::string key) {
    const char *surrounding = nullptr;
    size_t cursor = 0;
    const char *missing = "capability なし";
    const auto caps = ic_.capabilityFlags();
    const bool sensitive =
        caps.test(CapabilityFlag::Password) || caps.test(CapabilityFlag::Sensitive);
    if (sensitive) {
        missing = "sensitive";
    } else if (caps.test(CapabilityFlag::SurroundingText)) {
        const auto &s = ic_.surroundingText();
        missing = "invalid";
        if (s.isValid()) {
            surrounding = s.text().c_str();
            cursor = s.cursor();
        }
    }
    auto *response = rinze_client_key(client_.get(), key.c_str(), surrounding, cursor);
    if (engine_.debug()) {
        const char *left = "";
        auto origin = rinze_client_left_context(client_.get(), &left);
        diagnosis_ = surrounding ? std::string("左文脈 (") + leftContextName(origin) + "): " + left
                                 : std::string("左文脈 (") + missing + ")";
        RINZE_INFO() << diagnosis_ << " surrounding=\"" << (surrounding ? surrounding : "")
                     << "\" cursor=" << cursor;
    }
    return apply(response);
}

void RinzeState::flush() {
    if (!idle()) {
        apply(rinze_client_flush(client_.get()));
    }
}

void RinzeState::discard() {
    if (!idle()) {
        apply(rinze_client_discard(client_.get()));
    }
}

void RinzeState::switchMode(RinzeMode mode) {
    apply(rinze_client_switch_mode(client_.get(), mode));
}

void RinzeState::connect() {
    if (!shown_) {
        apply(rinze_client_discard(client_.get()));
    }
}

void RinzeState::startupFinished(RinzeStatus status, const char *message) {
    if (status == RINZE_STATUS_READY) {
        connect();
        return;
    }
    notice_ = message;
    render();
}

void RinzeState::restoreNotice() {
    if (!notice_.empty() && ic_.hasFocus() && ic_.inputPanel().auxUp().empty()) {
        render();
    }
}

bool RinzeState::apply(RinzeResponse *raw) {
    ResponsePtr response(raw);
    if (!response) {
        const char *message = "";
        auto failure = rinze_client_failure(client_.get(), &message);
        RINZE_WARN() << message;
        notice_ = message;
        if (failure == RINZE_FAILURE_DELAYED) {
            render();
            return true;
        }
        shown_.reset();
        render();
        if (failure == RINZE_FAILURE_STARTING) {
            engine_.watchStartup(ic_);
        }
        return false;
    }
    std::string commit = rinze_response_commit(response.get());
    if (!commit.empty()) {
        ic_.commitString(commit);
    }
    auto before = mode();
    bool handled = rinze_response_handled(response.get());
    shown_ = std::move(response);
    notice_.clear();
    render();
    if (mode() != before) {
        engine_.modeChanged(ic_, before.has_value());
    }
    return handled;
}

void RinzeState::render() {
    auto &panel = ic_.inputPanel();
    panel.reset();
    std::vector<std::string> aux;
    if (shown_) {
        Text preedit;
        preedit.append(rinze_response_preedit(shown_.get()),
                       {TextFormatFlag::Underline, TextFormatFlag::DontCommit});
        preedit.setCursor(static_cast<int>(preedit.textLength()));
        if (ic_.capabilityFlags().test(CapabilityFlag::Preedit)) {
            panel.setClientPreedit(preedit);
        } else {
            panel.setPreedit(preedit);
        }
        const char *prompt = nullptr;
        const char *text = nullptr;
        const char *typing = nullptr;
        if (rinze_response_minibuffer(shown_.get(), &prompt, &text, &typing)) {
            panel.setAuxUp(Text(std::string(prompt) + text + typing));
        }
        if (showsCandidates()) {
            panel.setCandidateList(std::make_unique<RinzePage>(*this, shown_.get()));
            aux.push_back("[" + std::to_string(rinze_response_page(shown_.get()) + 1) + "/" +
                          std::to_string(rinze_response_pages(shown_.get())) + "]");
        } else {
            std::string annotation = rinze_response_annotation(shown_.get());
            if (!annotation.empty()) {
                aux.push_back(std::move(annotation));
            }
        }
    }
    if (!notice_.empty()) {
        panel.setAuxUp(Text("rinze: " + notice_));
    }
    if (engine_.debug() && !diagnosis_.empty()) {
        aux.push_back(diagnosis_);
    }
    if (!aux.empty()) {
        std::string joined;
        for (const auto &part : aux) {
            joined += (joined.empty() ? "" : " ") + part;
        }
        panel.setAuxDown(Text(joined));
    }
    ic_.updatePreedit();
    ic_.updateUserInterface(UserInterfaceComponent::InputPanel);
}

class RinzeFactory final : public fcitx::AddonFactory {
    fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
        return new RinzeEngine(manager->instance());
    }
};

} // namespace

FCITX_ADDON_FACTORY_V2(rinze, RinzeFactory)
