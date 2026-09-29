use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

pub const COMMON_MEDIA_EXTS: &[&str] = &["flv", "mkv", "mp4", "avi", "rmvb", "m2ts", "wmv"];
pub const COMMON_CAPTION_EXTS: &[&str] = &["srt", "ass", "ssa", "sub", "smi", "xml"];
pub static COMMON_MEDIA_EXTS_SET: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    for &ext in COMMON_MEDIA_EXTS {
        set.insert(ext);
    }
    set
});

pub static COMMON_CAPTION_EXTS_SET: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    for &ext in COMMON_CAPTION_EXTS {
        set.insert(ext);
    }
    set
});

pub static KEEP_LANGS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("und");
    set.insert("zh");
    set.insert("zh-cn");
    set.insert("zh_cn");
    set.insert("chi");
    set.insert("chs");
    set.insert("zho");
    set.insert("cht");
    set.insert("zh-tw");
    set.insert("zh_tw");
    set.insert("hant");
    set.insert("zh-hans");
    set.insert("zh-hant");
    set.insert("srd");
    set
});

pub static RE_IGNORE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r" - S\d{1,3}E\d{1,3}(\.5)?\.").unwrap());
pub static RE_SXE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[Ss](\d{1,3})[Ee](\d{1,3}(\.5)?)").unwrap());
pub static RE_SXEP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[Ss](\d{1,3})[Ee][Pp](\d{1,3}(\.5)?)").unwrap());
pub static RE_YEAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(\d{4,4}\)").unwrap());
pub static RE_CHINESE_SEASON: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"第([一二三四五六七八九十壹贰叁肆伍陆柒捌玖拾]{1,3})季").unwrap());
pub static RE_CHINESE_EP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"第(\d{1,3}(\.5)?)[集话話]").unwrap());
pub static RE_EP: Lazy<Regex> = Lazy::new(|| Regex::new(r"[Ee](\d{1,3}(\.5)?)").unwrap());
pub static RE_EP_PAT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[Ee][Pp](\d{1,3}(\.5)?)").unwrap());
pub static RE_SE_EX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[Ss][Ee](\d{1,3})\.(\d{1,3})").unwrap());
pub static RE_S_E: Lazy<Regex> = Lazy::new(|| Regex::new(r"[Ss](\d{1,3})-(\d{1,3})").unwrap());
pub static RE_DASH_EP: Lazy<Regex> = Lazy::new(|| Regex::new(r"- (\d{1,3}(\.5)?) -").unwrap());
pub static RE_ENDING_V2: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d{1,4}(\.5)?)[Vv]?\d?").unwrap());
pub static RE_ENDING_PURE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{1,4}(\.5)?$").unwrap());
pub static RE_EP_RANGE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[(\d{1,4})-(\d{1,4})\]").unwrap());
