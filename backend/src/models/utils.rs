use crate::models::constants::*;
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::path::PathBuf;

const BRACKETS: [(&str, &str); 4] = [
    (r"\[", r"\]"),
    (r"\(", r"\)"),
    (r"【", r"】"),
    (r"「", r"」"),
];
static BRACKET_EPISODES: Lazy<Vec<Regex>> = Lazy::new(|| {
    let patterns = [
        r"(\d{1,3}(\.5)?)",
        r"第(\d{1,3}(\.5)?)集",
        r"第(\d{1,3}(\.5)?)话",
        r"第(\d{1,3}(\.5)?)話",
        r"[Ee][Pp](\d{1,3}(\.5)?)",
        r"[Ee](\d{1,3}(\.5)?)",
        r"[Ss][Pp](\d{1,3}(\.5)?)",
        r"(\d{1,3}(\.5)?)[Vv]?\d?",
    ];
    patterns
        .iter()
        .flat_map(|pattern| {
            BRACKETS.iter().map(move |(open, close)| {
                Regex::new(&format!("{}{}{}", open, pattern, close)).expect("episode regex")
            })
        })
        .collect()
});
static EPISODE_SPLIT: Lazy<Regex> = Lazy::new(|| {
    let mut parts: Vec<_> = BRACKETS
        .iter()
        .map(|(open, close)| format!("{}.*?{}", open, close))
        .collect();
    parts.push(r"\-".to_string());
    Regex::new(&parts.join("|")).expect("episode split regex")
});

pub fn chinese_to_number(s: &str) -> Option<i32> {
    let s = s.trim();

    if s.is_empty() {
        return None;
    }

    let digits = [
        ("零", 0),
        ("〇", 0),
        ("○", 0),
        ("一", 1),
        ("二", 2),
        ("三", 3),
        ("四", 4),
        ("五", 5),
        ("六", 6),
        ("七", 7),
        ("八", 8),
        ("九", 9),
        ("壹", 1),
        ("贰", 2),
        ("叁", 3),
        ("肆", 4),
        ("伍", 5),
        ("陆", 6),
        ("柒", 7),
        ("捌", 8),
        ("玖", 9),
    ];

    let tens = [("十", 10), ("拾", 10)];

    let mut result = 0;
    let mut has_ten = false;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let current_char = chars[i];
        let mut matched = false;

        for (ten_str, _ten_val) in tens.iter() {
            if ten_str.chars().next() == Some(current_char) {
                if i == 0 {
                    result += 10;
                } else {
                    result += 10;
                }
                has_ten = true;
                matched = true;
                i += 1;
                break;
            }
        }

        if !matched {
            for (digit_str, digit_val) in digits.iter() {
                if digit_str.chars().next() == Some(current_char) {
                    if has_ten && i > 0 {
                        result += *digit_val;
                    } else {
                        result = *digit_val;
                    }
                    matched = true;
                    i += 1;
                    break;
                }
            }
        }

        if !matched {
            i += 1;
        }
    }

    if result > 0 && result <= 100 {
        Some(result)
    } else {
        None
    }
}

#[derive(Debug)]
pub struct ParseSeasonEpisodeResult {
    pub season: Option<String>,
    pub ep: Option<String>,
}

/// 将全角数字转换为半角数字
fn fullwidth_to_halfwidth(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| matches!(c, '０'..='９')) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| {
                match c {
                    '０'..='９' => {
                        // 全角数字: U+FF10 到 U+FF19
                        // 半角数字: U+0030 到 U+0039
                        // 差值: 0xFF10 - 0x0030 = 0xFEE0
                        (c as u32 - 0xFEE0) as u8 as char
                    }
                    _ => c,
                }
            })
            .collect(),
    )
}

pub fn parse_season_episode(file_name: &str) -> ParseSeasonEpisodeResult {
    debug!("[parse_season_episode] 开始解析文件名: {:?}", file_name);

    // 将全角数字转换为半角数字
    let file_name = fullwidth_to_halfwidth(file_name);
    debug!(
        "[parse_season_episode] 全角数字转换后的文件名: {:?}",
        file_name
    );

    let upper_file_name = file_name.to_uppercase();
    let mut season: Option<String> = None;
    let mut ep: Option<String> = None;

    if RE_IGNORE.is_match(&file_name) {
        debug!("[parse_season_episode] 检测到已按规则命名的文件，尝试解析 season 和 ep");
        if let Some(captures) = RE_SXE.captures(&upper_file_name) {
            if let (Some(s), Some(e)) = (captures.get(1), captures.get(2)) {
                if let Ok(s_num) = s.as_str().parse::<i32>() {
                    season = Some(format!("{:02}", s_num));
                    let e_str = e.as_str();
                    if e_str.contains('.') {
                        ep = Some(e_str.to_string());
                    } else if let Ok(e_num) = e_str.parse::<i32>() {
                        ep = Some(format!("{:02}", e_num));
                    }
                    debug!(
                        "[parse_season_episode] 通过已命名格式匹配: season={:?}, ep={:?}",
                        season, ep
                    );
                    return ParseSeasonEpisodeResult { season, ep };
                }
            }
        }
    }

    if let Some(captures) = RE_SXE.captures(&upper_file_name) {
        if let (Some(s), Some(e)) = (captures.get(1), captures.get(2)) {
            if let Ok(s_num) = s.as_str().parse::<i32>() {
                season = Some(format!("{:02}", s_num));
                let e_str = e.as_str();
                if e_str.contains('.') {
                    ep = Some(e_str.to_string());
                } else if let Ok(e_num) = e_str.parse::<i32>() {
                    ep = Some(format!("{:02}", e_num));
                }
                debug!(
                    "[parse_season_episode] 通过SxE格式匹配: season={:?}, ep={:?}",
                    season, ep
                );
                return ParseSeasonEpisodeResult { season, ep };
            }
        }
    }

    if let Some(captures) = RE_SXEP.captures(&upper_file_name) {
        if let (Some(s), Some(e)) = (captures.get(1), captures.get(2)) {
            if let Ok(s_num) = s.as_str().parse::<i32>() {
                season = Some(format!("{:02}", s_num));
                let e_str = e.as_str();
                if e_str.contains('.') {
                    ep = Some(e_str.to_string());
                } else if let Ok(e_num) = e_str.parse::<i32>() {
                    ep = Some(format!("{:02}", e_num));
                }
                debug!(
                    "[parse_season_episode] 通过SxEP格式匹配: season={:?}, ep={:?}",
                    season, ep
                );
                return ParseSeasonEpisodeResult { season, ep };
            }
        }
    }

    if season.is_none() {
        if let Some(captures) = RE_CHINESE_SEASON.captures(&file_name) {
            if let Some(chinese_num) = captures.get(1) {
                if let Some(s_num) = chinese_to_number(chinese_num.as_str()) {
                    season = Some(format!("{:02}", s_num));
                    debug!(
                        "[parse_season_episode] 通过中文季数格式匹配: season={:?}",
                        season
                    );
                }
            }
        }
    }

    let filename_ny = RE_YEAR.replace(&file_name, "");

    for pat in BRACKET_EPISODES.iter() {
        if let Some(captures) = pat.captures(&filename_ny) {
            if let Some(e) = captures.get(1) {
                ep = Some(e.as_str().to_string());
                break;
            }
        }
    }

    if ep.is_none() {
        debug!("[parse_season_episode] 括号内未识别, 开始寻找括号外内容");

        let mut res: Vec<&str> = EPISODE_SPLIT
            .split(&filename_ny)
            .filter(|s| !s.is_empty())
            .collect();
        res.reverse();

        debug!("[parse_season_episode] 分割后的部分: {:?}", res);

        if ep.is_none() {
            for y in res.iter() {
                let y = y.trim();
                if let Some(captures) = RE_CHINESE_EP.captures(y) {
                    if let Some(e) = captures.get(1) {
                        ep = Some(e.as_str().to_string());
                        debug!("[parse_season_episode] 通过第x集格式匹配: ep={:?}", ep);
                        break;
                    }
                }
            }
        }

        if ep.is_none() {
            for y in res.iter() {
                let y = y.trim();
                if let Some(captures) = RE_EP_PAT.captures(&y.to_uppercase()) {
                    if let Some(e) = captures.get(1) {
                        ep = Some(e.as_str().to_string());
                        debug!("[parse_season_episode] 通过EP格式匹配: ep={:?}", ep);
                        break;
                    }
                }
            }
        }

        if ep.is_none() {
            for y in res.iter() {
                let y = y.trim();
                if let Some(captures) = RE_SE_EX.captures(&y.to_uppercase()) {
                    if let (Some(s), Some(e)) = (captures.get(1), captures.get(2)) {
                        if let Ok(s_num) = s.as_str().parse::<i32>() {
                            season = Some(format!("{:02}", s_num));
                        }
                        if let Ok(e_num) = e.as_str().parse::<i32>() {
                            ep = Some(format!("{:02}", e_num));
                        }
                        debug!(
                            "[parse_season_episode] 通过SE.x格式匹配: season={:?}, ep={:?}",
                            season, ep
                        );
                        break;
                    }
                }
            }
        }

        if ep.is_none() {
            let y = filename_ny.replace(" ", "");
            if let Some(captures) = RE_S_E.captures(&y.to_uppercase()) {
                if let (Some(s), Some(e)) = (captures.get(1), captures.get(2)) {
                    if let Ok(s_num) = s.as_str().parse::<i32>() {
                        season = Some(format!("{:02}", s_num));
                    }
                    if let Ok(e_num) = e.as_str().parse::<i32>() {
                        ep = Some(format!("{:02}", e_num));
                    }
                    debug!(
                        "[parse_season_episode] 通过Sx-Ey格式匹配: season={:?}, ep={:?}",
                        season, ep
                    );
                }
            }
        }

        if ep.is_none() {
            let y = filename_ny.trim();
            if let Some(captures) = RE_DASH_EP.captures(y) {
                if let Some(e) = captures.get(1) {
                    ep = Some(e.as_str().to_string());
                    debug!("[parse_season_episode] 通过- x -格式匹配: ep={:?}", ep);
                }
            }
        }

        if ep.is_none() {
            for y in res.iter() {
                let y = y.trim();
                if let Some(captures) = RE_EP.captures(&y.to_uppercase()) {
                    if let Some(e) = captures.get(1) {
                        ep = Some(e.as_str().to_string());
                        debug!("[parse_season_episode] 通过Ex格式匹配: ep={:?}", ep);
                        break;
                    }
                }
            }
        }

        if ep.is_none() {
            debug!("[parse_season_episode] 找末尾是数字的子字符串");
            for s in res.iter() {
                let s = s.trim();

                if let Some(captures) = RE_ENDING_V2.captures(s) {
                    if let Some(e) = captures.get(1) {
                        ep = Some(e.as_str().to_string());
                        debug!(
                            "[parse_season_episode] 通过末尾数字(v2/.5)匹配: ep={:?}",
                            ep
                        );
                        break;
                    }
                }

                if ep.is_none() {
                    if let Some(captures) = RE_ENDING_PURE.captures(s) {
                        ep = Some(captures.get(0).unwrap().as_str().to_string());
                        debug!("[parse_season_episode] 通过末尾纯数字匹配: ep={:?}", ep);
                        break;
                    }
                }
            }
        }

        if ep.is_none() {
            if let Some(captures) = RE_CHINESE_EP.captures(&filename_ny) {
                if let Some(e) = captures.get(1) {
                    ep = Some(e.as_str().to_string());
                    debug!("[parse_season_episode] 通过最后第x集格式匹配: ep={:?}", ep);
                }
            }
        }
    }

    if ep.is_some() {
        debug!(
            "[parse_season_episode] 最终解析结果: season={:?}, ep={:?}",
            season, ep
        );
    } else {
        debug!("[parse_season_episode] 未能解析到任何集数");
    }

    ParseSeasonEpisodeResult { season, ep }
}

/// 获取项目根目录的绝对路径
pub fn get_project_root() -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        return cwd;
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            return parent.to_path_buf();
        }
    }
    PathBuf::from(".")
}

/// 获取 webui/images 目录的绝对路径
pub fn get_webui_images_dir() -> PathBuf {
    let root = get_project_root();
    let path = root.join("webui").join("images");
    debug!("[路径] 工作目录: {:?}", root);
    debug!("[路径] 图片目录: {:?}", path);
    path
}
