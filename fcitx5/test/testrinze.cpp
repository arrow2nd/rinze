// testfrontend で打鍵を送り、利用者に見える振る舞い (確定、未確定文字列、パネル、モードの表示) を確かめる。
// 場面ごとに別のプロセスで走らせる。rinzed の用意は test/run.sh がする。
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <csignal>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <memory>
#include <sstream>
#include <string>
#include <string_view>
#include <thread>
#include <vector>

#include <fcitx-utils/event.h>
#include <fcitx-utils/eventdispatcher.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/log.h>
#include <fcitx-utils/macros.h>
#include <fcitx-utils/testing.h>
#include <fcitx/action.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/inputmethodgroup.h>
#include <fcitx/inputmethodmanager.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterfacemanager.h>

#include "testfrontend_public.h"

using namespace fcitx;

namespace {

using Clock = std::chrono::steady_clock;

double millisSince(Clock::time_point started) {
    return std::chrono::duration<double, std::milli>(Clock::now() - started).count();
}

std::string env(const char *name) {
    const char *value = std::getenv(name);
    FCITX_ASSERT(value) << name << " が無い";
    return value;
}

bool contains(const std::string &text, std::string_view part) {
    return text.find(part) != std::string::npos;
}

struct Timings {
    explicit Timings(std::string name) : name(std::move(name)) {}

    std::string name;
    std::vector<double> millis;

    void report() {
        if (millis.empty()) {
            return;
        }
        std::sort(millis.begin(), millis.end());
        FCITX_INFO() << "所要時間 " << name << ": " << millis.size() << " 回、中央値 "
                     << millis[millis.size() / 2] << " ms、最小 " << millis.front()
                     << " ms、最大 " << millis.back() << " ms";
    }
};

class Typist {
public:
    explicit Typist(Instance *instance, const std::string &program = "testapp")
        : instance_(instance), frontend_(instance->addonManager().addon("testfrontend")) {
        uuid_ = frontend_->call<ITestFrontend::createInputContext>(program);
        ic_ = instance->inputContextManager().findByUUID(uuid_);
        ic_->setCapabilityFlags(
            CapabilityFlags{CapabilityFlag::Preedit, CapabilityFlag::SurroundingText});
    }

    void destroy() { frontend_->call<ITestFrontend::destroyInputContext>(uuid_); }

    InputContext *ic() { return ic_; }

    bool press(const Key &key) {
        auto started = Clock::now();
        bool accepted = frontend_->call<ITestFrontend::sendKeyEvent>(uuid_, key, false);
        lastMillis = millisSince(started);
        return accepted;
    }
    bool press(const char *name) { return press(Key(name)); }

    void type(std::string_view keys) {
        for (char c : keys) {
            FCITX_ASSERT(press(Key(static_cast<KeySym>(c)))) << "rinze が受け取る: " << c;
        }
    }

    void enableRinze() {
        FCITX_ASSERT(press("Control+space"));
        FCITX_ASSERT(instance_->inputMethod(ic_) == "rinze") << instance_->inputMethod(ic_);
    }

    void expectCommit(const std::string &text) {
        frontend_->call<ITestFrontend::pushCommitExpectation>(text);
    }

    void setSurrounding(const std::string &text, unsigned int cursor) {
        ic_->surroundingText().setText(text, cursor, cursor);
        ic_->updateSurroundingText();
    }

    std::string preedit() { return ic_->inputPanel().clientPreedit().toString(); }
    std::string auxUp() { return ic_->inputPanel().auxUp().toString(); }
    std::string auxDown() { return ic_->inputPanel().auxDown().toString(); }
    std::string modeLabel() { return instance_->inputMethodLabel(ic_); }
    std::shared_ptr<CandidateList> candidates() { return ic_->inputPanel().candidateList(); }

    std::string shownCandidates() {
        std::string shown;
        if (auto list = candidates()) {
            for (int i = 0; i < list->size(); i++) {
                shown += list->label(i).toString() + list->candidate(i).text().toString() + " ";
            }
        }
        return shown;
    }

    std::string converted() {
        auto shown = preedit();
        FCITX_ASSERT(shown.rfind("▼", 0) == 0) << shown;
        return shown.substr(std::string("▼").size());
    }

    void cancelAll() {
        for (int i = 0; i < 4 && !preedit().empty(); i++) {
            press("Control+g");
        }
        FCITX_ASSERT(preedit().empty()) << preedit();
    }

    double lastMillis = 0;

private:
    Instance *instance_;
    AddonInstance *frontend_;
    ICUUID uuid_;
    InputContext *ic_;
};

void useRinze(Instance *instance) {
    FCITX_ASSERT(instance->addonManager().addon("rinze", true));
    auto group = instance->inputMethodManager().currentGroup();
    group.inputMethodList().clear();
    group.inputMethodList().push_back(InputMethodGroupItem("keyboard-us"));
    group.inputMethodList().push_back(InputMethodGroupItem("rinze"));
    group.setDefaultInputMethod("");
    instance->inputMethodManager().setGroup(group);
}

// イベントループを回しながら、条件が満たされるまで待つ
class Waiter {
public:
    void until(Instance *instance, double timeoutMillis, std::function<bool()> done,
               std::function<void(bool, double)> then) {
        auto started = Clock::now();
        timer_ = instance->eventLoop().addTimeEvent(
            CLOCK_MONOTONIC, now(CLOCK_MONOTONIC) + 20'000, 0,
            [=](EventSourceTime *source, uint64_t) {
                bool ok = done();
                auto waited = millisSince(started);
                if (!ok && waited < timeoutMillis) {
                    source->setNextInterval(20'000);
                    source->setOneShot();
                    return true;
                }
                then(ok, waited);
                return true;
            });
    }

private:
    std::unique_ptr<EventSourceTime> timer_;
};

size_t socketsOf(const std::string &pid) {
    size_t count = 0;
    for (const auto &fd : std::filesystem::directory_iterator("/proc/" + pid + "/fd")) {
        std::error_code error;
        auto target = std::filesystem::read_symlink(fd.path(), error);
        if (!error && target.string().rfind("socket:", 0) == 0) {
            count++;
        }
    }
    return count;
}

std::string readFile(const std::string &path) {
    std::ifstream in(path);
    std::stringstream text;
    text << in.rdbuf();
    return text.str();
}

void typing(Instance *instance) {
    useRinze(instance);
    Typist t(instance);
    t.enableRinze();
    FCITX_ASSERT(t.modeLabel() == "かな") << t.modeLabel();
    Timings keys{"変換以外の打鍵"};
    Timings spaces{"Space での変換"};

    // 見出し、変換、確定
    for (char c : std::string_view("Asita")) {
        FCITX_ASSERT(t.press(Key(static_cast<KeySym>(c))));
        keys.millis.push_back(t.lastMillis);
    }
    FCITX_ASSERT(t.preedit() == "▽あした") << t.preedit();
    FCITX_ASSERT(t.press("space"));
    spaces.millis.push_back(t.lastMillis);
    FCITX_ASSERT(t.preedit() == "▼明日") << t.preedit();
    t.expectCommit("明日");
    FCITX_ASSERT(t.press("Return"));
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();

    // 次の文字で確定し、その文字はかなとして打つ
    t.type("Asita");
    t.press("space");
    t.expectCommit("明日");
    t.expectCommit("は");
    t.type("ha");
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();

    // 送りあり
    t.type("Oku");
    FCITX_ASSERT(t.press("R"));
    FCITX_ASSERT(t.preedit() == "▽おく*r") << t.preedit();
    t.type("u");
    auto okuri = t.converted();
    FCITX_ASSERT(okuri.size() > 3 && okuri.substr(okuri.size() - 3) == "る") << okuri;
    t.expectCommit(okuri);
    t.press("Return");

    // 左文脈で 1 位が変わる
    for (auto [left, first] : {std::pair{"精密な", "機械"}, std::pair{"絶好の", "機会"}}) {
        t.setSurrounding(left, 3);
        t.type("Kikai");
        t.press("space");
        spaces.millis.push_back(t.lastMillis);
        FCITX_ASSERT(t.converted() == first) << left << " のあと: " << t.converted();
        FCITX_ASSERT(contains(t.auxDown(), std::string("左文脈 (surrounding): ") + left))
            << t.auxDown();
        t.cancelAll();
    }

    // Firefox のように未確定文字列の混ざった surrounding text から、未確定文字列だけを除く
    t.type("Kika");
    t.setSurrounding("絶好の▽きか", 6);
    t.type("i");
    FCITX_ASSERT(contains(t.auxDown(), "左文脈 (未確定文字列を除いた): 絶好の")) << t.auxDown();
    t.cancelAll();
    t.type("A");
    t.setSurrounding("記号▽", 3);
    t.type("s");
    FCITX_ASSERT(contains(t.auxDown(), "左文脈 (surrounding): 記号▽")) << t.auxDown();
    t.cancelAll();
    t.ic()->surroundingText().invalidate();
    t.type("A");
    FCITX_ASSERT(contains(t.auxDown(), "左文脈 (invalid)")) << t.auxDown();
    t.cancelAll();
    t.ic()->setCapabilityFlags(CapabilityFlags{CapabilityFlag::Preedit});
    t.type("A");
    FCITX_ASSERT(contains(t.auxDown(), "左文脈 (capability なし)")) << t.auxDown();
    t.cancelAll();
    t.ic()->setCapabilityFlags(
        CapabilityFlags{CapabilityFlag::Preedit, CapabilityFlag::SurroundingText,
                        CapabilityFlag::Sensitive});
    t.setSurrounding("秘密の文脈", 5);
    t.type("A");
    FCITX_ASSERT(contains(t.auxDown(), "左文脈 (sensitive)")) << t.auxDown();
    t.cancelAll();
    t.ic()->setCapabilityFlags(
        CapabilityFlags{CapabilityFlag::Preedit, CapabilityFlag::SurroundingText,
                        CapabilityFlag::Password});
    FCITX_ASSERT(!t.press("a")) << "パスワード欄では打鍵を rinze が奪わずアプリへ通す";
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();
    t.ic()->setCapabilityFlags(
        CapabilityFlags{CapabilityFlag::Preedit, CapabilityFlag::SurroundingText});

    // 候補一覧: 5 候補目から一覧になり、選択キーとクリックで選べる
    auto openList = [&] {
        t.type("Kou");
        for (int i = 0; i < 5; i++) {
            t.press("space");
        }
        auto list = t.candidates();
        FCITX_ASSERT(list && list->size() == 7) << t.shownCandidates();
        FCITX_ASSERT(list->label(0).toString() == "a:" && list->label(6).toString() == "l:")
            << t.shownCandidates();
        FCITX_ASSERT(t.auxDown().rfind("[1/", 0) == 0) << t.auxDown();
        return list;
    };
    auto list = openList();
    FCITX_INFO() << "候補一覧: " << t.shownCandidates() << t.auxDown();
    t.expectCommit(list->candidate(1).text().toString());
    FCITX_ASSERT(t.press("s"));
    FCITX_ASSERT(t.preedit().empty() && !t.candidates()) << t.preedit();
    list = openList();
    t.expectCommit(list->candidate(2).text().toString());
    list->candidate(2).select(t.ic());
    FCITX_ASSERT(t.preedit().empty() && !t.candidates()) << t.preedit();

    // ページ送り: rinze のキー (Space、x)、fcitx5 のページ送りのキー (既定は Down、Up)、パネルの矢印
    list = openList();
    auto firstOfPage = [&] { return t.candidates()->candidate(0).text().toString(); };
    auto page1 = firstOfPage();
    FCITX_ASSERT(t.press("Down"));
    FCITX_ASSERT(t.auxDown().rfind("[2/", 0) == 0) << t.auxDown();
    auto page2 = firstOfPage();
    FCITX_ASSERT(page2 != page1);
    FCITX_ASSERT(t.press("space"));
    FCITX_ASSERT(t.auxDown().rfind("[3/", 0) == 0) << t.auxDown();
    FCITX_ASSERT(t.press("Up"));
    FCITX_ASSERT(firstOfPage() == page2);
    FCITX_ASSERT(t.press("x"));
    FCITX_ASSERT(firstOfPage() == page1);
    t.candidates()->toPageable()->next();
    FCITX_ASSERT(firstOfPage() == page2) << "パネルの次のページ";
    t.candidates()->toPageable()->prev();
    FCITX_ASSERT(firstOfPage() == page1) << "パネルの前のページ";
    FCITX_ASSERT(t.press("x"));
    FCITX_ASSERT(!t.candidates() && t.preedit().rfind("▼", 0) == 0) << "最初のページの x は一覧を閉じる";
    t.cancelAll();

    // 辞書登録の入力欄
    t.type("Rinze");
    for (int i = 0; i < 30 && !contains(t.auxUp(), "[辞書登録]"); i++) {
        t.press("space");
    }
    FCITX_ASSERT(t.auxUp() == "[辞書登録] りんぜ: ") << t.auxUp();
    FCITX_ASSERT(t.preedit() == "▼りんぜ") << t.preedit();
    t.type("ka");
    FCITX_ASSERT(t.auxUp() == "[辞書登録] りんぜ: か") << t.auxUp();
    FCITX_ASSERT(t.press("l"));
    FCITX_ASSERT(t.modeLabel() == "aA") << t.modeLabel();
    FCITX_ASSERT(t.auxUp() == "[辞書登録] りんぜ: か") << "モードの知らせで入力欄を隠さない: " << t.auxUp();
    FCITX_ASSERT(t.press("Control+j"));
    FCITX_ASSERT(t.auxUp() == "[辞書登録] りんぜ: か") << t.auxUp();
    t.type("K");
    FCITX_ASSERT(t.auxUp() == "[辞書登録] りんぜ: か▽k") << t.auxUp();
    t.cancelAll();
    FCITX_ASSERT(t.auxUp().empty()) << t.auxUp();

    // 候補を辞書から消すかの問い
    t.type("Asita");
    t.press("space");
    t.type("X");
    FCITX_ASSERT(t.auxUp() == "あした /明日/ を辞書から削除します。良いですか？ (yes or no) ")
        << t.auxUp();
    t.type("no");
    FCITX_ASSERT(contains(t.auxUp(), "(yes or no) no")) << t.auxUp();
    t.press("Return");
    FCITX_ASSERT(t.auxUp().empty() && t.preedit() == "▼明日") << t.preedit();
    t.cancelAll();

    // 5 つの入力モードとサブモードの表示
    auto *modeAction = instance->userInterfaceManager().lookupAction("rinze-mode");
    FCITX_ASSERT(modeAction);
    auto statusActions = t.ic()->statusArea().allActions();
    FCITX_ASSERT(std::find(statusActions.begin(), statusActions.end(), modeAction) !=
                 statusActions.end());
    struct ModeStep {
        const char *key;
        const char *label;
        const char *typed;
        const char *committed;
    };
    for (auto step : {ModeStep{"q", "カナ", "ka", "カ"}, ModeStep{"q", "かな", "ka", "か"},
                      ModeStep{"Control+q", "ｶﾅ", "ka", "ｶ"},
                      ModeStep{"Control+q", "かな", "ka", "か"},
                      ModeStep{"L", "全英", "a", "ａ"}, ModeStep{"Control+j", "かな", "a", "あ"},
                      ModeStep{"l", "aA", "", ""}}) {
        FCITX_ASSERT(t.press(step.key)) << step.key;
        FCITX_ASSERT(t.modeLabel() == step.label) << step.key << ": " << t.modeLabel();
        FCITX_ASSERT(modeAction->shortText(t.ic()).rfind(step.label, 0) == 0)
            << modeAction->shortText(t.ic());
        if (*step.typed) {
            t.expectCommit(step.committed);
            t.type(step.typed);
        }
    }

    // ASCII モードでは ctrl-j 以外を rinzed に送らない。止めた rinzed に送れば待つので、待たないことで確かめる
    auto rinzedPid = env("RINZE_TEST_RINZED_PID");
    FCITX_ASSERT(kill(std::stoi(rinzedPid), SIGSTOP) == 0);
    for (const char *key : {"a", "A", "space", "Return", "BackSpace", "Escape", "Control+g"}) {
        FCITX_ASSERT(!t.press(key)) << key << " はアプリへ通す";
        FCITX_ASSERT(t.lastMillis < 100) << key << ": " << t.lastMillis << " ms";
    }
    FCITX_ASSERT(kill(std::stoi(rinzedPid), SIGCONT) == 0);
    FCITX_ASSERT(t.auxUp().rfind("rinze:", 0) != 0) << "rinzed を待たないので知らせも出ない: " << t.auxUp();
    FCITX_ASSERT(t.press("Control+j"));
    FCITX_ASSERT(t.modeLabel() == "かな") << t.modeLabel();

    // トレイのメニューからモードを選ぶ
    auto *fullwidth = instance->userInterfaceManager().lookupAction("rinze-mode-fullwidth-latin");
    FCITX_ASSERT(fullwidth && !fullwidth->isChecked(t.ic()));
    fullwidth->activate(t.ic());
    FCITX_ASSERT(t.modeLabel() == "全英" && fullwidth->isChecked(t.ic())) << t.modeLabel();
    instance->userInterfaceManager().lookupAction("rinze-mode-halfwidth-katakana")->activate(t.ic());
    FCITX_ASSERT(t.modeLabel() == "ｶﾅ") << t.modeLabel();
    t.type("Asi");
    t.expectCommit("ｱｼ");
    instance->userInterfaceManager().lookupAction("rinze-mode-hiragana")->activate(t.ic());
    FCITX_ASSERT(t.modeLabel() == "かな" && t.preedit().empty()) << t.modeLabel();

    // Ctrl のキーは大文字でも小文字でも受け、Escape は C-g と同じに 1 段ずつ戻す
    for (const char *ctrlG : {"Control+g", "Control+G"}) {
        t.type("Asita");
        FCITX_ASSERT(t.press(ctrlG));
        FCITX_ASSERT(t.preedit().empty()) << ctrlG << ": " << t.preedit();
    }
    for (auto [ctrl, committed] : {std::pair{"Control+j", "あした"}, std::pair{"Control+J", "あした"},
                                   std::pair{"Control+q", "ｱｼﾀ"}, std::pair{"Control+Q", "ｱｼﾀ"}}) {
        t.type("Asita");
        t.expectCommit(committed);
        FCITX_ASSERT(t.press(ctrl)) << ctrl;
    }
    FCITX_ASSERT(t.modeLabel() == "かな") << t.modeLabel();
    t.type("Asita");
    t.press("space");
    FCITX_ASSERT(t.press("Escape"));
    FCITX_ASSERT(t.preedit() == "▽あした") << t.preedit();
    FCITX_ASSERT(t.press("Escape"));
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();
    FCITX_ASSERT(!t.press("Escape")) << "入力途中が無ければアプリへ通す";

    // 矢印キーは表示どおりに確定してからアプリへ通す。BackSpace は入力途中があれば rinze が使う
    t.type("Asita");
    t.press("space");
    t.expectCommit("明日");
    FCITX_ASSERT(!t.press("Left"));
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();
    FCITX_ASSERT(!t.press("Left"));
    t.type("Asita");
    FCITX_ASSERT(t.press("BackSpace"));
    FCITX_ASSERT(t.preedit() == "▽あし") << t.preedit();
    t.cancelAll();
    FCITX_ASSERT(!t.press("BackSpace"));

    // フォーカスが外れると入力途中を確定せずに捨て、学習にも書かず、モードは残す
    auto history = env("RINZE_TEST_HISTORY");
    t.press("q");
    t.type("Asita");
    t.press("space");
    t.press("space");
    auto second = t.converted();
    t.ic()->focusOut();
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();
    FCITX_ASSERT(!contains(readFile(history), "あした\t")) << readFile(history);
    t.ic()->focusIn();
    FCITX_ASSERT(t.modeLabel() == "カナ") << t.modeLabel();
    t.expectCommit("カ");
    t.type("ka");
    t.type("Asita");
    t.press("space");
    t.press("space");
    t.expectCommit(t.converted());
    t.press("Return");
    FCITX_ASSERT(contains(readFile(history), "あした\t" + second))
        << "送って確定した候補は学習に書く: " << readFile(history);
    t.press("q");

    // アプリの reset でも入力途中を捨てる
    t.type("Asita");
    t.ic()->reset();
    FCITX_ASSERT(t.preedit().empty()) << t.preedit();

    // 入力メソッドを切り替えると、表示どおりに確定する
    t.type("Asita");
    t.press("space");
    t.expectCommit("明日");
    FCITX_ASSERT(t.press("Control+space"));
    FCITX_ASSERT(instance->inputMethod(t.ic()) == "keyboard-us");
    FCITX_ASSERT(t.press("Control+space"));

    // 打鍵の所要時間
    t.setSurrounding("", 0);
    for (int round = 0; round < 4; round++) {
        for (auto [key, kana] : {std::pair{'a', "あ"}, std::pair{'i', "い"}, std::pair{'u', "う"},
                                 std::pair{'e', "え"}, std::pair{'o', "お"}}) {
            t.expectCommit(kana);
            t.press(Key(static_cast<KeySym>(key)));
            keys.millis.push_back(t.lastMillis);
        }
    }

    // 使われなくなった入力欄の接続を閉じる
    auto before = socketsOf(rinzedPid);
    std::vector<std::unique_ptr<Typist>> others;
    for (int i = 0; i < 10; i++) {
        others.push_back(std::make_unique<Typist>(instance, "other" + std::to_string(i)));
        others.back()->enableRinze();
        others.back()->expectCommit("あ");
        others.back()->type("a");
    }
    // rinzed は接続ごとに読む側と書く側の 2 つの記述子を持つ
    auto opened = socketsOf(rinzedPid);
    FCITX_ASSERT(opened == before + 2 * others.size()) << before << " から " << opened;
    for (auto &other : others) {
        other->destroy();
    }
    size_t closed = opened;
    for (int i = 0; i < 100 && closed != before; i++) {
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
        closed = socketsOf(rinzedPid);
    }
    FCITX_ASSERT(closed == before) << "入力欄を閉じたあとの rinzed の接続: " << closed;
    FCITX_INFO() << "rinzed のソケット: 入力欄 10 個で " << before << " から " << opened
                 << "、閉じて " << closed;

    // 応答が遅くても 10 秒までは待って変換する
    auto pid = std::stoi(rinzedPid);
    auto resumeAfter = [pid](int millis) {
        return std::thread([pid, millis] {
            std::this_thread::sleep_for(std::chrono::milliseconds(millis));
            kill(pid, SIGCONT);
        });
    };
    t.type("Asita");
    FCITX_ASSERT(kill(pid, SIGSTOP) == 0);
    auto resume = resumeAfter(1500);
    FCITX_ASSERT(t.press("space"));
    resume.join();
    FCITX_INFO() << "1.5 秒止めた rinzed: Space " << t.lastMillis << " ms";
    FCITX_ASSERT(t.preedit() == "▼明日" && t.lastMillis > 1400)
        << t.preedit() << " " << t.lastMillis << " ms";

    // 10 秒待っても応答が無ければ、未確定文字列を残し、打鍵はアプリへ通さない。ほかの入力欄はその後 1 秒で諦める
    FCITX_ASSERT(kill(pid, SIGSTOP) == 0);
    FCITX_ASSERT(t.press("Return"));
    FCITX_INFO() << "止めた rinzed: Return " << t.lastMillis << " ms";
    FCITX_ASSERT(t.lastMillis > 9900 && t.lastMillis < 11000) << t.lastMillis;
    FCITX_ASSERT(t.preedit() == "▼明日") << t.preedit();
    FCITX_ASSERT(t.auxUp() == "rinze: rinzed が 10 秒以内に応答しなかった。入力途中のものは残してあり、"
                              "次の打鍵で応答を待ち直す")
        << t.auxUp();
    Typist stalled(instance, "stalled");
    stalled.enableRinze();
    FCITX_ASSERT(stalled.lastMillis < 1500) << stalled.lastMillis;
    // fcitx5 5.1.19 は有効にした直後のパネルの上段に入力メソッドの名前を出すので、知らせは次の打鍵で確かめる
    FCITX_ASSERT(!stalled.press("a")) << "打鍵はアプリへ通す";
    FCITX_ASSERT(stalled.lastMillis < 1500) << stalled.lastMillis;
    FCITX_ASSERT(stalled.auxUp() == "rinze: rinzed が 1 秒以内に応答しなかった") << stalled.auxUp();
    stalled.destroy();

    // rinzed が戻れば、次の打鍵で遅れた応答を読み、そこで確定したものも失わない
    resume = resumeAfter(300);
    t.expectCommit("明日あ");
    FCITX_ASSERT(t.press("a"));
    resume.join();
    FCITX_ASSERT(t.lastMillis < 1000) << t.lastMillis;
    FCITX_ASSERT(t.preedit().empty() && t.auxUp().rfind("rinze:", 0) != 0)
        << t.preedit() << " " << t.auxUp();

    keys.report();
    spaces.report();
    t.destroy();
    instance->exit();
}

void autostart(Instance *instance, Waiter &waiter) {
    useRinze(instance);
    auto *t = new Typist(instance);
    auto started = Clock::now();
    t->enableRinze();
    FCITX_INFO() << "rinze を有効にする打鍵 (rinzed を起動する): " << t->lastMillis << " ms";
    FCITX_ASSERT(t->lastMillis < 200) << t->lastMillis;
    // fcitx5 5.1.19 は有効にした直後のパネルの上段に入力メソッドの名前を出すので、知らせは次の打鍵で確かめる
    FCITX_ASSERT(!t->press("a")) << "起動を待つ間の打鍵はアプリへ通す";
    FCITX_ASSERT(t->lastMillis < 100) << t->lastMillis;
    FCITX_ASSERT(t->auxUp() == "rinze: rinzed を起動しています") << t->auxUp();
    waiter.until(
        instance, 30'000, [t] { return t->modeLabel() == "かな"; },
        [=](bool ready, double) {
            FCITX_ASSERT(ready) << "rinzed が待ち受けを始めない: " << t->auxUp();
            FCITX_INFO() << "rinze を有効にしてから rinzed に繋がって表示が戻るまで: "
                         << millisSince(started) << " ms";
            FCITX_ASSERT(t->auxUp().rfind("rinze:", 0) != 0) << t->auxUp();
            t->expectCommit("か");
            t->type("ka");
            t->destroy();
            delete t;
            instance->exit();
        });
}

void missing(Instance *instance) {
    useRinze(instance);
    Typist t(instance);
    t.enableRinze();
    FCITX_ASSERT(!t.press("a")) << "打鍵はアプリへ通す";
    FCITX_ASSERT(t.lastMillis < 100) << t.lastMillis;
    FCITX_ASSERT(t.auxUp() == "rinze: rinzed が PATH に無い") << t.auxUp();
    t.destroy();
    instance->exit();
}

// rinzed が起動してすぐ失敗した (モデルが無いなど) ときは、打鍵が無くても標準エラー出力の理由をパネルに出す
void failing(Instance *instance, Waiter &waiter) {
    useRinze(instance);
    auto *t = new Typist(instance);
    auto started = Clock::now();
    t->enableRinze();
    FCITX_ASSERT(!t->press("a")) << "打鍵はアプリへ通す";
    auto exited = std::string("rinze: ") + env("RINZE_RINZED") + " が終了した (exit status: 1): ";
    // 知らせが出たあとに fcitx5 が入力メソッドの名前を出しても (5.1.19 はパネルの上段を上書きし、1 秒後に空にする)、知らせは戻る
    auto informed = std::make_shared<bool>(false);
    waiter.until(
        instance, 30'000,
        [=] {
            if (!contains(t->auxUp(), "が終了した")) {
                return false;
            }
            if (!*informed) {
                FCITX_INFO() << "rinzed の起動の失敗をパネルに出すまで: " << millisSince(started) << " ms";
                instance->showInputMethodInformation(t->ic());
                *informed = true;
                return false;
            }
            return true;
        },
        [=](bool shown, double waited) {
            FCITX_ASSERT(shown && t->auxUp().rfind(exited, 0) == 0) << t->auxUp();
            FCITX_ASSERT(contains(t->auxUp(), "(exit status: 1): rinzed: モデル /missing.gguf を読めない")) << t->auxUp();
            FCITX_INFO() << "入力メソッドの名前を出したあと、知らせが戻るまで含めて: " << waited << " ms: " << t->auxUp();
            t->destroy();
            delete t;
            instance->exit();
        });
}

void unresponsive(Instance *instance) {
    auto path = env("RINZE_SOCKET");
    int server = socket(AF_UNIX, SOCK_STREAM, 0);
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    path.copy(address.sun_path, sizeof(address.sun_path) - 1);
    FCITX_ASSERT(bind(server, reinterpret_cast<sockaddr *>(&address), sizeof(address)) == 0);
    FCITX_ASSERT(listen(server, 16) == 0);

    useRinze(instance);
    Typist t(instance);
    t.enableRinze();
    FCITX_INFO() << "応答しない rinzed: rinze を有効にする打鍵 " << t.lastMillis << " ms";
    FCITX_ASSERT(t.lastMillis > 9900 && t.lastMillis < 11000) << t.lastMillis;
    // 一度待ち切れなかったあとは、打鍵ごとに 1 秒で諦めてアプリへ通す
    for (const char *notice : {"rinzed が応答しないので、入力途中のものを捨てた",
                               "rinzed が 1 秒以内に応答しなかった"}) {
        FCITX_ASSERT(!t.press("a")) << "打鍵はアプリへ通す";
        FCITX_INFO() << "応答しない rinzed: 打鍵 " << t.lastMillis << " ms";
        FCITX_ASSERT(t.lastMillis > 900 && t.lastMillis < 2000) << t.lastMillis;
        FCITX_ASSERT(t.auxUp() == std::string("rinze: ") + notice) << t.auxUp();
    }
    t.destroy();
    close(server);
    instance->exit();
}

} // namespace

int main(int argc, char *argv[]) {
    FCITX_ASSERT(argc == 2) << "使い方: testrinze typing|autostart|missing|failing|unresponsive";
    std::string scenario = argv[1];
    setupTestingEnvironment(TESTING_BINARY_DIR, {"addon-lib"}, {"data"});
    char arg0[] = "testrinze";
    char arg1[] = "--disable=all";
    char arg2[] = "--enable=testim,testfrontend,rinze";
    char *args[] = {arg0, arg1, arg2};
    Log::setLogRule("default=5,rinze=5");
    Instance instance(FCITX_ARRAY_SIZE(args), args);
    instance.addonManager().registerDefaultLoader(nullptr);
    Waiter waiter;
    instance.eventDispatcher().schedule([&] {
        if (scenario == "typing") {
            typing(&instance);
        } else if (scenario == "autostart") {
            autostart(&instance, waiter);
        } else if (scenario == "missing") {
            missing(&instance);
        } else if (scenario == "failing") {
            failing(&instance, waiter);
        } else if (scenario == "unresponsive") {
            unresponsive(&instance);
        } else {
            FCITX_ASSERT(false) << "未知の場面: " << scenario;
        }
    });
    instance.exec();
    return 0;
}
