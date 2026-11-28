//! Path-prefix scoping over `metering::View` for isolated multi-account homes.
use super::View;
use crate::pricing::PriceRegions;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) struct HomeScope {
    own_prefixes: Vec<PathBuf>,
    excluded_prefixes: Vec<PathBuf>,
}

pub(crate) enum Attribution {
    Provider,
    Home(HomeScope),
    Ambiguous,
}

impl crate::AgentHudCore {
    pub(crate) fn attribution(&self, vendor: &str, id: &serde_json::Value) -> Attribution {
        let accounts: Vec<_> = self
            .accounts
            .accounts
            .iter()
            .filter(|account| account["provider"] == vendor)
            .collect();
        let configured = self.settings[format!("{vendor}Homes")]
            .as_array()
            .is_some_and(|homes| !homes.is_empty());
        if accounts.len() <= 1 && !configured {
            return Attribution::Provider;
        }
        let home = |account: &serde_json::Value| {
            account["home"]
                .as_str()
                .filter(|home| !home.is_empty())
                .map(str::to_owned)
        };
        let Some(own) = accounts
            .iter()
            .find(|account| account["id"] == *id)
            .and_then(|account| home(account))
        else {
            return Attribution::Ambiguous;
        };
        let mut others = Vec::new();
        for account in accounts.iter().filter(|account| account["id"] != *id) {
            let Some(other) = home(account) else {
                return Attribution::Ambiguous;
            };
            if !HomeScope::are_distinct(&own, &other) {
                return Attribution::Ambiguous;
            }
            others.push(other);
        }
        Attribution::Home(HomeScope::from_homes(&own, &others))
    }
}

impl HomeScope {
    pub fn from_homes(own: &str, all_provider_homes: &[String]) -> Self {
        let own_path = Path::new(own);
        let mut own_prefixes = path_variants(own_path);
        // Default `~/.claude` also covers XDG `~/.config/claude` when not overridden.
        if own_path.file_name().is_some_and(|name| name == ".claude") {
            if let Some(user_home) = own_path.parent() {
                own_prefixes.extend(path_variants(&user_home.join(".config/claude")));
            }
        }
        let mut excluded_prefixes = Vec::new();
        for other in all_provider_homes {
            if !Self::are_distinct(own, other) {
                continue;
            }
            for variant in path_variants(Path::new(other)) {
                if !excluded_prefixes.contains(&variant) {
                    excluded_prefixes.push(variant);
                }
            }
        }
        Self {
            own_prefixes,
            excluded_prefixes,
        }
    }

    pub fn are_distinct(a: &str, b: &str) -> bool {
        let ca = canonical_or_clean(Path::new(a));
        let cb = canonical_or_clean(Path::new(b));
        ca != cb
    }

    pub fn matches_contribution(&self, contribution: &str) -> bool {
        let file_str = contribution
            .split_once("#hud-session-")
            .map_or(contribution, |(prefix, _)| prefix);
        let path = Path::new(file_str);
        let canon = fs::canonicalize(path).ok();
        let in_own = self.own_prefixes.iter().any(|prefix| {
            path.starts_with(prefix) || canon.as_ref().is_some_and(|c| c.starts_with(prefix))
        });
        if !in_own {
            return false;
        }
        !self.excluded_prefixes.iter().any(|excluded| {
            // Only exclude a path if `excluded` is not an ancestor of `own`.
            let excluded_contains_own = self
                .own_prefixes
                .iter()
                .any(|own| own != excluded && own.starts_with(excluded));
            !excluded_contains_own
                && (path.starts_with(excluded)
                    || canon.as_ref().is_some_and(|c| c.starts_with(excluded)))
        })
    }
}

fn canonical_or_clean(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.components().collect())
}

fn path_variants(path: &Path) -> Vec<PathBuf> {
    let clean: PathBuf = path.components().collect();
    let mut out = vec![clean];
    if let Ok(canon) = fs::canonicalize(path) {
        if !out.contains(&canon) {
            out.push(canon);
        }
    }
    out
}

impl View {
    /// Builds a lightweight view scoped strictly to contributions inside `scope`.
    pub(crate) fn scoped(&self, scope: &HomeScope, regions: &PriceRegions, now: i64) -> Self {
        let parts = self
            .parts
            .iter()
            .filter(|(path, _)| scope.matches_contribution(path))
            .map(|(path, part)| (path.clone(), part.clone()))
            .collect();
        let mut scoped = Self {
            keys: self.keys.clone(),
            key_index: self.key_index.clone(),
            parts,
            ..Self::default()
        };
        scoped.aggregate(regions);
        scoped.refresh_clock(now);
        scoped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_matches_own_home_and_excludes_nested_or_sibling_homes() {
        let temp = tempfile::tempdir().unwrap();
        let user = temp.path().join("u");
        let default_home = user.join(".claude");
        let work_home = user.join(".claude-work");
        let nested_home = default_home.join("nested-profile");
        let all = vec![
            default_home.to_string_lossy().into_owned(),
            work_home.to_string_lossy().into_owned(),
            nested_home.to_string_lossy().into_owned(),
        ];

        let default_scope = HomeScope::from_homes(&all[0], &all);
        assert!(default_scope
            .matches_contribution(&default_home.join("projects/p1/a.jsonl").to_string_lossy()));
        assert!(default_scope.matches_contribution(
            &user
                .join(".config/claude/projects/p2/b.jsonl")
                .to_string_lossy()
        ));
        assert!(!default_scope
            .matches_contribution(&work_home.join("projects/p1/c.jsonl").to_string_lossy()));
        assert!(!default_scope
            .matches_contribution(&nested_home.join("projects/p1/d.jsonl").to_string_lossy()));

        let nested_scope = HomeScope::from_homes(&all[2], &all);
        assert!(nested_scope
            .matches_contribution(&nested_home.join("projects/p1/d.jsonl").to_string_lossy()));
        assert!(!nested_scope
            .matches_contribution(&default_home.join("projects/p1/a.jsonl").to_string_lossy()));
    }
}
