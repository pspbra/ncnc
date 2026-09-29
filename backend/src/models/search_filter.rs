use crate::models::config::FilterTerm;
use crate::models::constants::RE_EP_RANGE;
use crate::models::jackett::JackettResult;
use crate::models::utils::parse_season_episode;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Episode,
    TvdbEpisode,
    Absolute,
}

/// 检查标题是否匹配可选搜索名
pub fn matches_optional_name(title: &str, optional_terms: &[FilterTerm]) -> bool {
    for term in optional_terms {
        match term {
            FilterTerm::Single(term_str) => {
                if title.contains(term_str) {
                    return true;
                }
            }
            FilterTerm::Multiple(term_array) => {
                let mut all_contain = true;
                for term_str in term_array {
                    if !title.contains(term_str) {
                        all_contain = false;
                        break;
                    }
                }
                if all_contain {
                    return true;
                }
            }
        }
    }
    false
}

pub trait SearchResult {
    fn title(&self) -> Option<&str>;
    fn size(&self) -> Option<u64>;
    fn category_desc(&self) -> Option<&str>;
}

impl SearchResult for JackettResult {
    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn size(&self) -> Option<u64> {
        self.size
    }

    fn category_desc(&self) -> Option<&str> {
        self.category_desc.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct FilteredResult<T> {
    pub result: T,
    pub is_multi_episode: bool,
}

/// 同一标题的解析、合集范围及标准化结果可以在多个集数匹配之间复用。
pub struct PreparedTitle {
    has_range: bool,
    range: Option<(i32, i32)>,
    episode: Option<i32>,
    season: Option<i32>,
    pub normalized: String,
}

impl PreparedTitle {
    pub fn new(title: &str) -> Self {
        let captures = RE_EP_RANGE.captures(title);
        let range = captures.as_ref().and_then(|caps| {
            Some((
                caps.get(1)?.as_str().parse().ok()?,
                caps.get(2)?.as_str().parse().ok()?,
            ))
        });
        let parsed = parse_season_episode(title);
        let episode = parsed
            .ep
            .as_deref()
            .and_then(|ep| ep.parse::<f64>().ok())
            .map(|ep| ep as i32);
        let season = match parsed.season.as_deref() {
            None | Some("") => Some(1),
            Some(season) => season.parse().ok(),
        };
        Self {
            has_range: captures.is_some(),
            range,
            episode,
            season,
            normalized: normalize_name(title),
        }
    }

    pub fn matches<T: SearchResult>(
        &self,
        result: &T,
        target_ep: i32,
        search_type: SearchType,
        season_number: i32,
        tvdb_season_number: Option<i32>,
        normalized_season_name: Option<&str>,
        filter_terms: Option<&[FilterTerm]>,
    ) -> Option<bool> {
        let title = result.title()?;
        if !self.has_range && !title.contains(&target_ep.to_string()) {
            return None;
        }
        if result.size().is_some_and(|size| size > 2_500_000_000)
            || result.category_desc().is_some_and(|category| {
                let category = category.to_lowercase();
                category.contains("audio")
                    || category.contains("books")
                    || category.contains("software")
            })
        {
            return None;
        }
        let is_multi = self
            .range
            .is_some_and(|(start, end)| target_ep >= start && target_ep <= end);
        if !is_multi && self.episode != Some(target_ep) {
            return None;
        }
        let has_season_name =
            normalized_season_name.is_some_and(|name| self.normalized.contains(name));
        let season_matches = has_season_name
            || match search_type {
                SearchType::Absolute => true,
                SearchType::Episode => self.season == Some(season_number),
                SearchType::TvdbEpisode => {
                    self.season.is_some() && self.season == tvdb_season_number
                }
            };
        if !season_matches {
            return None;
        }
        if filter_terms.is_some_and(|terms| {
            terms.iter().any(|term| match term {
                FilterTerm::Single(term) => title.contains(term),
                FilterTerm::Multiple(terms) => terms.iter().all(|term| title.contains(term)),
            })
        }) {
            return None;
        }
        Some(is_multi)
    }
}

pub fn normalize_season_name(name: &str) -> String {
    normalize_name(&name.replace("篇", ""))
}

pub fn filter_search_results<T: SearchResult + Send + Sync + Clone>(
    results: Vec<T>,
    target_ep: i32,
    search_type: SearchType,
    season_number: i32,
    tvdb_season_number: Option<i32>,
    season_name: Option<&str>,
    filter_terms: Option<&[FilterTerm]>,
) -> Vec<FilteredResult<T>> {
    let normalized_season = season_name.map(normalize_season_name);
    let episode_text = target_ep.to_string();
    let select = |result: T| {
        let title = result.title()?;
        if result.size().is_some_and(|size| size > 2_500_000_000)
            || result.category_desc().is_some_and(|category| {
                let category = category.to_lowercase();
                category.contains("audio")
                    || category.contains("books")
                    || category.contains("software")
            })
            || (!title.contains(&episode_text) && !RE_EP_RANGE.is_match(title))
        {
            return None;
        }
        let prepared = PreparedTitle::new(title);
        let is_multi_episode = prepared.matches(
            &result,
            target_ep,
            search_type,
            season_number,
            tvdb_season_number,
            normalized_season.as_deref(),
            filter_terms,
        )?;
        Some(FilteredResult {
            result,
            is_multi_episode,
        })
    };
    if results.len() < 64 {
        results.into_iter().filter_map(select).collect()
    } else {
        results.into_par_iter().filter_map(select).collect()
    }
}

pub fn calculate_priority_score(title: &str) -> i32 {
    let mut score = 0;
    let priority_keywords_1 = ["字幕组", "ANi", "喵萌奶茶屋", "雪飘工作室"];
    let priority_keywords_2 = ["简日", "字幕", "CHT", "CHS", "简体", "繁体", "简繁"];

    for keyword in priority_keywords_1 {
        if title.contains(keyword) {
            score += 1;
        }
    }
    for keyword in priority_keywords_2 {
        if title.contains(keyword) {
            score += 1;
        }
    }
    score
}

/// 判断是否是其他常见标点符号
pub fn normalize_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut separator = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if separator && !result.is_empty() {
                result.push(' ');
            }
            result.push(c.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    result
}
