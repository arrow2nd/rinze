use rmpv::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Hiragana,
    Katakana,
    HalfwidthKatakana,
    Ascii,
    FullwidthLatin,
}

impl Mode {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "hiragana" => Mode::Hiragana,
            "katakana" => Mode::Katakana,
            "halfwidth-katakana" => Mode::HalfwidthKatakana,
            "ascii" => Mode::Ascii,
            "fullwidth-latin" => Mode::FullwidthLatin,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePage {
    pub items: Vec<(char, String)>,
    pub page: usize,
    pub pages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minibuffer {
    pub prompt: String,
    pub text: String,
    pub preedit: String,
}

/// rinzed の応答。各項目の意味は README の「通信」の節にある。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub handled: bool,
    pub commit: String,
    pub preedit: String,
    pub candidates: Option<CandidatePage>,
    pub minibuffer: Option<Minibuffer>,
    pub mode: Mode,
    pub passthrough: bool,
    pub version: u64,
}

type Map = [(Value, Value)];

fn field<'a>(map: &'a Map, name: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| k.as_str() == Some(name))
        .map(|(_, v)| v)
        .filter(|v| !v.is_nil())
}

fn string(map: &Map, name: &str) -> Result<String, String> {
    match field(map, name) {
        None => Ok(String::new()),
        Some(v) => v
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{name} が文字列でない")),
    }
}

fn number(map: &Map, name: &str) -> Result<u64, String> {
    field(map, name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} が数でない"))
}

fn flag(map: &Map, name: &str) -> bool {
    field(map, name).and_then(Value::as_bool).unwrap_or(false)
}

impl Response {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let map = value.as_map().ok_or("応答が map でない")?;
        let mode = field(map, "mode")
            .and_then(Value::as_str)
            .and_then(Mode::parse)
            .ok_or("応答の mode が分からない")?;
        let candidates = match field(map, "candidates") {
            None => None,
            Some(c) => Some(CandidatePage::parse(
                c.as_map().ok_or("candidates が map でない")?,
            )?),
        };
        let minibuffer = match field(map, "minibuffer") {
            None => None,
            Some(m) => {
                let m = m.as_map().ok_or("minibuffer が map でない")?;
                Some(Minibuffer {
                    prompt: string(m, "prompt")?,
                    text: string(m, "text")?,
                    preedit: string(m, "preedit")?,
                })
            }
        };
        Ok(Response {
            handled: flag(map, "handled"),
            commit: string(map, "commit")?,
            preedit: string(map, "preedit")?,
            candidates,
            minibuffer,
            mode,
            passthrough: flag(map, "passthrough"),
            version: number(map, "version")?,
        })
    }

    pub fn is_idle(&self) -> bool {
        self.preedit.is_empty() && self.candidates.is_none() && self.minibuffer.is_none()
    }
}

impl CandidatePage {
    fn parse(map: &Map) -> Result<Self, String> {
        let items = field(map, "items")
            .and_then(Value::as_array)
            .ok_or("candidates.items が配列でない")?
            .iter()
            .map(|item| match item.as_array().map(Vec::as_slice) {
                Some([label, word]) => {
                    let mut label = label.as_str().unwrap_or_default().chars();
                    match (label.next(), label.next(), word.as_str()) {
                        (Some(label), None, Some(word)) => Ok((label, word.to_string())),
                        _ => Err("候補の選択キーが 1 文字でない"),
                    }
                }
                _ => Err("候補が [選択キー, 候補] でない"),
            })
            .collect::<Result<_, _>>()?;
        Ok(CandidatePage {
            items,
            page: number(map, "page")? as usize,
            pages: number(map, "pages")? as usize,
        })
    }
}
