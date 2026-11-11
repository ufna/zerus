//! Whether two write sets touch the same files.
//!
//! Imported from WhiteWh/zerus at `ca4cc4481` (`src/state/teams/model.rs`:
//! `Task::output_overlaps` and `Task::paths_overlap`) without the fork's
//! hard-coded list of folders whose paths only advise. Here every path holds.

/// Whether any path of `a` contains, equals or lies inside any path of `b`.
/// An empty write set overlaps nothing.
pub fn overlaps(a: &[String], b: &[String]) -> bool {
    let b: Vec<Vec<&str>> = b.iter().map(|path| components(path)).collect();
    a.iter()
        .map(|path| components(path))
        .any(|a| b.iter().any(|b| contains(&a, b)))
}

/// Whether one path contains the other, compared by components, so
/// `src/api` never contains `src/apis`.
pub fn paths_overlap(a: &str, b: &str) -> bool {
    contains(&components(a), &components(b))
}

fn contains(a: &[&str], b: &[&str]) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// A path as the folders it lies in, read so that no spelling hides an
/// overlap: `/` and `\` both separate, `.` and empty parts drop out, `..`
/// steps back, and a pattern counts as the folder it starts in. A path that
/// leaves the repository counts as the whole repository, which is no parts.
fn components(path: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Vec::new();
                }
            }
            _ if part.contains(['*', '?', '[', '{']) => break,
            _ => parts.push(part),
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn a_folder_holds_what_is_inside_it_and_nothing_beside_it() {
        assert!(paths_overlap("src/api", "src/api/routes.rs"));
        assert!(paths_overlap("src/api/routes.rs", "src/api"));
        assert!(paths_overlap("src/api/routes.rs", "src/api/routes.rs"));
        assert!(!paths_overlap("src/api", "src/apis"));
        assert!(!paths_overlap("src/api/a.rs", "src/api/b.rs"));
    }

    #[test]
    fn spelling_does_not_hide_an_overlap() {
        assert!(paths_overlap("./src/a.rs", "src/a.rs"));
        assert!(paths_overlap("src//a.rs", "src/a.rs"));
        assert!(paths_overlap("src/", "src/a.rs"));
        assert!(paths_overlap("/src/a.rs", "src/a.rs"));
        assert!(paths_overlap("src\\a.rs", "src/a.rs"));
        assert!(paths_overlap("src/x/../a.rs", "src/a.rs"));
        assert!(!paths_overlap("src/x/../a.rs", "src/x/a.rs"));
    }

    #[test]
    fn the_whole_repository_overlaps_everything() {
        for whole in ["", ".", "/", "./", "../elsewhere", "src/../../x"] {
            assert!(paths_overlap(whole, "docs/readme.md"), "{whole:?}");
        }
    }

    #[test]
    fn a_pattern_holds_the_folder_it_starts_in() {
        assert!(paths_overlap("src/teams/*", "src/teams/cli.rs"));
        assert!(paths_overlap("src/**/mod.rs", "src/teams/mod.rs"));
        assert!(paths_overlap(
            "client/src/ui/*.tsx",
            "client/src/ui/teams/Board.tsx"
        ));
        assert!(paths_overlap("crates/{a,b}/lib.rs", "crates/c/lib.rs"));
        assert!(!paths_overlap("client/*.ts", "src/main.rs"));
    }

    #[test]
    fn a_top_level_area_is_not_only_advice() {
        // The fork let `client`, `crates/replica` or `src` pass as advice.
        assert!(paths_overlap("client", "client/src/app.tsx"));
        assert!(paths_overlap(
            "crates/replica",
            "crates/replica/src/task.rs"
        ));
        assert!(paths_overlap("src", "src/main.rs"));
        assert!(paths_overlap("Makefile", "Makefile"));
    }

    #[test]
    fn sets_overlap_when_any_pair_does() {
        assert!(overlaps(&set(&["docs/a.md", "src/x.rs"]), &set(&["src"])));
        assert!(!overlaps(&set(&["docs/a.md"]), &set(&["src", "client"])));
        assert!(!overlaps(&[], &set(&["src"])));
        assert!(!overlaps(&set(&["src"]), &[]));
    }
}
