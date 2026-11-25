//! Interface text the app draws natively (tray and menu-bar items, their
//! tooltip): the webview's generated catalog, read once. The language is the
//! saved Settings > Language at startup; a change applies after a restart, as
//! in the webview. Verbatim arguments (names, versions) are never translated.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use zerus_client_core::text::{Arg, Text};

static RUSSIAN: AtomicBool = AtomicBool::new(false);
static CATALOG: LazyLock<serde_json::Map<String, serde_json::Value>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/i18n.ru.json")).unwrap_or_default()
});

/// Settings' `workspace/language` at startup: "ru" selects Russian.
pub fn configure(language: &str) {
    RUSSIAN.store(language == "ru", Ordering::Relaxed);
}

/// A core Text in the interface language, nested prose included.
pub fn render(text: &Text) -> String {
    render_in(text, RUSSIAN.load(Ordering::Relaxed))
}

/// Lines of a tooltip, each rendered, joined by line breaks.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn lines(texts: &[Text]) -> String {
    texts.iter().map(render).collect::<Vec<_>>().join("\n")
}

fn render_in(text: &Text, russian: bool) -> String {
    let args: Vec<String> = text
        .args
        .iter()
        .map(|arg| match arg {
            Arg::Text(nested) => render_in(nested, russian),
            Arg::Verbatim(value) => value.clone(),
            Arg::Number(value) => value.to_string(),
        })
        .collect();
    let count = text.args.first().and_then(|arg| match arg {
        Arg::Number(value) => Some(*value),
        _ => None,
    });
    let template = if russian {
        translation(&text.source, count)
    } else {
        None
    };
    fill(template.as_deref().unwrap_or(&text.source), &args)
}

/// The Russian template, choosing a plural form by the first numeric argument.
fn translation(source: &str, count: Option<i64>) -> Option<String> {
    match CATALOG.get(source)? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(forms) => {
            let form = russian_form(count.unwrap_or(0));
            forms.get(form)?.as_str().map(str::to_owned)
        }
        _ => None,
    }
}

fn russian_form(count: i64) -> usize {
    let n = count.unsigned_abs();
    if n % 10 == 1 && n % 100 != 11 {
        0
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        1
    } else {
        2
    }
}

/// One pass over the template, as the webview's `translateWithCatalog`
/// (`%L?<digits>` or `%n`): an argument containing `%1` is never expanded
/// again, and a missing argument leaves its token as written.
fn fill(template: &str, args: &[String]) -> String {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let (mut at, mut copied) = (0, 0);
    while at < bytes.len() {
        if bytes[at] != b'%' {
            at += 1;
            continue;
        }
        let mut end = at + 1;
        let value = if bytes.get(end) == Some(&b'n') {
            end += 1;
            args.first()
        } else {
            if bytes.get(end) == Some(&b'L') {
                end += 1;
            }
            let digits = end;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            template[digits..end]
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .and_then(|index| args.get(index))
        };
        match value {
            Some(value) => {
                out.push_str(&template[copied..at]);
                out.push_str(value);
                copied = end;
                at = end;
            }
            None => at += 1,
        }
    }
    out.push_str(&template[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests pass the language explicitly: the process-wide choice is shared
    // with every other test of the app.
    #[test]
    fn english_fills_arguments_and_russian_uses_the_catalog() {
        let open = zerus_client_core::text!("Update available: Zerus %1", "0.48.0");
        assert_eq!(render_in(&open, false), "Update available: Zerus 0.48.0");
        assert_eq!(
            render_in(&Text::verbatim("100% done %1"), false),
            "100% done %1"
        );
        assert_eq!(render_in(&open, true), "Доступно обновление: Zerus 0.48.0");
        // Verbatim values stay as they are, and are never expanded again.
        assert_eq!(render_in(&Text::verbatim("a %1 b"), true), "a %1 b");
        assert_eq!(
            render_in(
                &zerus_client_core::text!("%1%2", "studio", zerus_client_core::text!("  (paused)")),
                true
            ),
            "studio  (пауза)"
        );
        assert_eq!(
            render_in(&zerus_client_core::text!("%1 need attention", 3_i64), true),
            "Требуют внимания: 3"
        );
        let summary = zerus_client_core::text!(
            "%1 active, %2",
            4_i64,
            zerus_client_core::text!(
                "%1%2",
                zerus_client_core::text!("%1 need attention", 3_i64),
                zerus_client_core::text!(", %1 sessions total", 55_i64)
            )
        );
        assert_eq!(
            render_in(&summary, true),
            "Активно: 4, Требуют внимания: 3, всего сессий: 55"
        );
    }

    #[test]
    fn every_native_team_prompt_source_has_russian_words_and_the_same_arguments() {
        let source = include_str!("../../core/src/desktop/team_confirmation.rs");
        let mut sources = zerus_client_core::text::marked_sources(source);
        sources.extend(zerus_client_core::text::marked_sources(include_str!(
            "../../core/src/teams/action/input.rs"
        )));
        sources.extend(zerus_client_core::text::marked_sources(include_str!(
            "desktop_service/team_person.rs"
        )));
        sources.extend(zerus_client_core::text::marked_sources(include_str!(
            "desktop_service/team_person_actions.rs"
        )));
        sources.sort_unstable();
        sources.dedup();
        assert!(sources.len() >= 150);
        let args = (1..=9).map(|n| format!("[value{n}]")).collect::<Vec<_>>();
        for source in sources {
            let translated = translation(&source, None)
                .unwrap_or_else(|| panic!("missing native Teams translation for {source:?}"));
            let en = fill(&source, &args);
            let ru = fill(&translated, &args);
            for arg in &args {
                assert_eq!(en.matches(arg).count(), ru.matches(arg).count(), "{source}");
            }
            assert!(!en.contains('{') && !ru.contains('{'), "{source}");
        }
    }

    /// Review of #2558 (P2): the native formatter's own Stop, Archive and
    /// Remove sentences in English and Russian, beside the window's
    /// confirmation for the same call. The window's fallback dialog test
    /// (`TeamConfirmDialog.native.render.test.ts`) shows that confirmation and
    /// compares it with these renderings. The last row is the window's own
    /// unnamed-team message, rendered here by the same renderer.
    /// `ZERUS_UPDATE_NATIVE_CONFIRMATION_FIXTURE=1` rewrites the fixture.
    #[test]
    fn native_confirmations_are_the_window_fixture() {
        use zerus_client_core::{
            desktop::{
                creation_clone::Scope,
                team_confirmation::{self, Labels},
                team_person_actions as wire,
            },
            teams::action::{TeamAction, TeamCall},
        };
        const TEAM: &str = "11111111-1111-4111-8111-111111111111";
        let facts = "f".repeat(64);
        let labels = Labels {
            facts: facts.clone(),
            ..Default::default()
        };
        let mut rows = Vec::new();
        for action in [
            TeamAction::Stop {},
            TeamAction::Archive {},
            TeamAction::ArchiveMember {
                member: "m2".into(),
            },
        ] {
            let call = TeamCall {
                team: TEAM.into(),
                team_name: "Orchard".into(),
                action,
                expected_revision: Some(3),
                confirmed: false,
                request_id: "22222222-2222-4222-8222-222222222222".into(),
            };
            let plan = wire::Plan::from_call(&call).unwrap();
            let review = wire::Review {
                intent: wire::Intent {
                    scope: Scope {
                        configuration: "a".repeat(64),
                        service_epoch: "33333333-3333-4333-8333-333333333333".into(),
                    },
                    machine: "44444444-4444-4444-8444-444444444444".into(),
                    facts: facts.clone(),
                    request: "c".repeat(64),
                    store: wire::StoreBinding::Existing {
                        fingerprint: "d".repeat(64),
                    },
                    plan: plan.reference().unwrap(),
                },
                team_name: call.team_name.clone(),
                title: "Legacy title".into(),
                subject: "legacy".into(),
                details: Vec::new(),
            };
            let prompt = team_confirmation::action(&plan, &review, &labels).unwrap();
            rows.push(serde_json::json!({
                "from": "native prompt",
                "call": { "team_name": call.team_name, "action": call.action },
                "confirmation": call.action.confirmation(&call.team_name).unwrap(),
                "rendered": {
                    "en": render_in(&prompt.body[0], false),
                    "ru": render_in(&prompt.body[0], true),
                },
            }));
        }
        let unnamed = TeamAction::Stop {}.confirmation("").unwrap();
        rows.push(serde_json::json!({
            "from": "window message",
            "call": { "team_name": "", "action": TeamAction::Stop {} },
            "rendered": {
                "en": render_in(&unnamed.message, false),
                "ru": render_in(&unnamed.message, true),
            },
            "confirmation": unnamed,
        }));
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/ui/teams/fixtures/native-confirmations.json"
        );
        let text = serde_json::to_string_pretty(&rows).unwrap() + "\n";
        if std::env::var_os("ZERUS_UPDATE_NATIVE_CONFIRMATION_FIXTURE").is_some() {
            std::fs::write(path, &text).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(path).unwrap_or_default(),
            text,
            "regenerate with ZERUS_UPDATE_NATIVE_CONFIRMATION_FIXTURE=1"
        );
    }

    #[test]
    fn both_native_languages_render_member_removal_and_keep_reasons_quoted() {
        use zerus_client_core::{
            desktop::{
                creation_clone::Scope,
                team_confirmation::{self, Labels},
                team_person_actions as wire,
            },
            teams::action::{TeamAction, TeamCall},
        };
        const TEAM: &str = "11111111-1111-4111-8111-111111111111";
        let call = TeamCall {
            team: TEAM.into(),
            team_name: "Renderer caption".into(),
            action: TeamAction::ArchiveMember {
                member: "m2".into(),
            },
            expected_revision: Some(3),
            confirmed: false,
            request_id: "22222222-2222-4222-8222-222222222222".into(),
        };
        let plan = wire::Plan::from_call(&call).unwrap();
        let facts = "f".repeat(64);
        let review = wire::Review {
            intent: wire::Intent {
                scope: Scope {
                    configuration: "a".repeat(64),
                    service_epoch: "33333333-3333-4333-8333-333333333333".into(),
                },
                machine: "44444444-4444-4444-8444-444444444444".into(),
                facts: facts.clone(),
                request: "c".repeat(64),
                store: wire::StoreBinding::Existing {
                    fingerprint: "d".repeat(64),
                },
                plan: plan.reference().unwrap(),
            },
            team_name: "Orchard".into(),
            title: "Legacy title".into(),
            subject: "archive member".into(),
            details: vec![("legacy".into(), format!("{{\"team\":\"{TEAM}\"}}"))],
        };
        let labels = Labels {
            facts,
            member: Some("Reviewer".into()),
            ..Default::default()
        };
        let prompt = team_confirmation::action(&plan, &review, &labels).unwrap();
        assert_eq!(render_in(&prompt.title, false), "Remove");
        assert_eq!(render_in(&prompt.title, true), "Удалить");
        let en = render_in(&prompt.body[0], false);
        let ru = render_in(&prompt.body[0], true);
        assert!(en.starts_with("Remove Reviewer (m2) from Orchard?"), "{en}");
        assert!(en.contains("cannot be undone"), "{en}");
        assert!(
            ru.starts_with("Удалить Reviewer (m2) из команды Orchard?"),
            "{ru}"
        );
        assert!(ru.contains("нельзя отменить"), "{ru}");
        assert_eq!(
            render_in(&prompt.body[1], true),
            "Незавершённая работа вернётся лиду."
        );

        let plan = wire::Plan::from_call(&TeamCall {
            action: TeamAction::Cancel {
                node: 7,
                reason: "Duplicate.\nSee «context»\u{2028}in the report.".into(),
            },
            ..call
        })
        .unwrap();
        let mut review = review;
        review.intent.plan = plan.reference().unwrap();
        let prompt = team_confirmation::action(&plan, &review, &labels).unwrap();
        assert_eq!(
            render_in(&prompt.body[1], false),
            "Reason — “Duplicate. See \\«context\\» in the report.”"
        );
        assert_eq!(
            render_in(&prompt.body[1], true),
            "Причина — «Duplicate. See \\«context\\» in the report.»"
        );
    }

    #[test]
    fn filling_matches_the_webview_tokens() {
        let args = ["a".to_owned(), "b".to_owned()];
        assert_eq!(fill("%2-%1 %L1 %n %3 50%", &args), "b-a a a %3 50%");
        assert_eq!(fill("%1", &["%2".to_owned(), "x".to_owned()]), "%2");
    }

    #[test]
    fn russian_plural_forms_follow_the_webview_rule() {
        assert_eq!(
            [1, 2, 5, 11, 21, 22, 25, 111, 112].map(russian_form),
            [0, 1, 2, 2, 0, 1, 2, 2, 2]
        );
    }
}
