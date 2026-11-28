//! Read-only evidence for an unconfirmed stop. No result can reopen its
//! cleanup, mint a confirmation or reconstruct the program holder.
use super::*;
use zerus_client_core::worktree_programs::{
    stop::{Input, Report, State},
    MAX_BYTES,
};

pub(super) struct Unconfirmed {
    pub(super) original: Option<Box<Input>>,
}

#[derive(Serialize)]
pub(super) struct Availability {
    pub(super) can_check: bool,
    pub(super) has_request: bool,
}

pub(super) fn check_entry<'a>(model: &Model, job: &'a Job) -> Result<&'a Entry, String> {
    let entry = job.cleanup.entries.first().ok_or(STATE_CHANGED)?;
    let stop = job.unconfirmed_stop.as_ref().ok_or(STATE_CHANGED)?;
    if job.dismissed
        || job.busy
        || model.unknown_folder(job)
        || entry.status != Some(Status::Kept)
        || !entry.host.is_empty()
        || !zerus_client_core::creation::canonical_uuid(&entry.machine)
        || !zerus_client_core::creation::canonical_uuid(&entry.expected_owner_id)
        || entry.row["path"].as_str().is_none_or(str::is_empty)
        || entry.common_dir.is_empty()
        || stop.original.as_ref().is_some_and(|input| {
            let capture = &input.manifest.capture;
            capture.machine != entry.machine
                || Some(capture.root.as_str()) != entry.row["path"].as_str()
                || capture.common_dir != entry.common_dir
                || capture.owner_id != entry.expected_owner_id
                || job.source.as_ref() != Some(&capture.target)
                || job
                    .archive
                    .as_ref()
                    .is_none_or(|archive| archive.archive_id != input.archive_id)
        })
    {
        return Err(STATE_CHANGED.into());
    }
    Ok(entry)
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Recorded {
    RequestUnavailable,
    ReceiptMissing,
    StatusUnavailable,
    Unconfirmed,
    Complete,
    Incomplete,
    Refused,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    found: bool,
    report: Option<Report>,
}

pub(super) fn status(original: &Input) -> Recorded {
    status_with(original, |invocation| {
        run_limited(&default_hgs(), invocation, &[], MAX_BYTES + 1024)
            .unwrap_or_else(|_| Exit::StartFailed(STATE_CHANGED.into()))
    })
}

fn status_with(original: &Input, run: impl FnOnce(&Invocation) -> Exit) -> Recorded {
    let Ok(bytes) = serde_json::to_vec(original) else {
        return Recorded::StatusUnavailable;
    };
    if !original.valid() {
        return Recorded::StatusUnavailable;
    }
    let invocation = Invocation {
        args: vec![
            "worktrees".into(),
            "program-stop-status".into(),
            "--json".into(),
        ],
        stdin: Some(bytes),
        timeout: Duration::from_secs(8),
        request_id: None,
    };
    let Exit::Code {
        code: 0, stdout, ..
    } = run(&invocation)
    else {
        return Recorded::StatusUnavailable;
    };
    match serde_json::from_slice::<Reply>(&stdout) {
        Ok(Reply {
            found: false,
            report: None,
        }) => Recorded::ReceiptMissing,
        Ok(Reply {
            found: true,
            report: Some(report),
        }) if report.valid(original) => match report.state {
            State::Complete => Recorded::Complete,
            State::Incomplete => Recorded::Incomplete,
            State::Refused => Recorded::Refused,
            State::Unknown | State::Started => Recorded::Unconfirmed,
        },
        _ => Recorded::StatusUnavailable,
    }
}

pub(super) fn inspect(
    inner: &Inner,
    id: &str,
    recover: bool,
    status: impl FnOnce(&Input) -> Recorded,
    inspect: impl FnOnce(&str, &str, &str, &str) -> Result<Value, String>,
) -> Result<Value, String> {
    let scope = |model: &Model| {
        let job = model
            .jobs
            .iter()
            .find(|job| job.id == id)
            .ok_or(STATE_CHANGED)?;
        let entry = check_entry(model, job)?;
        Ok::<_, String>((
            entry.machine.clone(),
            entry.row["path"].as_str().ok_or(STATE_CHANGED)?.to_owned(),
            entry.common_dir.clone(),
            entry.expected_owner_id.clone(),
            job.unconfirmed_stop
                .as_ref()
                .and_then(|stop| stop.original.clone()),
        ))
    };
    let original = scope(&lock(&inner.model))?;
    let (machine, path, common, owner, request) = &original;
    let result = recover.then(|| {
        request
            .as_deref()
            .map_or(Recorded::RequestUnavailable, status)
    });
    let inspection = inspect(machine, path, common, owner).ok().filter(|value| {
        value["path"] == *path && value["common_dir"] == *common && value["review"].is_object()
    });
    if scope(&lock(&inner.model))? != original {
        return Err(STATE_CHANGED.into());
    }
    Ok(json!({
        "path": path,
        "common_dir": common,
        "review": inspection.as_ref().and_then(|value| value.get("review")),
        "inspection_failed": inspection.is_none(),
        "program_stop_result": result,
    }))
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use zerus_client_core::worktree_programs::{
        stop, DirectoryId, Manifest, ABI, MANIFEST_SECONDS,
    };

    fn unconfirmed() -> Inner {
        let inner = Inner::default();
        let mut job = super::super::stopping_job();
        let entry = &mut job.cleanup.entries[0];
        entry.host.clear();
        entry.expected_owner_id = uuid::Uuid::new_v4().to_string();
        let result = Err(program_pipeline::Unconfirmed {
            held: None,
            original: None,
        });
        assert!(!program_pipeline::record_stop(&mut job, result));
        lock(&inner.model).jobs.push_back(job);
        inner
    }

    fn original(inner: &Inner) -> Input {
        let mut model = lock(&inner.model);
        let job = &mut model.jobs[0];
        let entry = &mut job.cleanup.entries[0];
        let target = Target {
            computer: entry.host.clone(),
            name: "codex/example/stop".into(),
            run_id: "original-run".into(),
            conversation_id: "original-conversation".into(),
        };
        job.source = Some(target.clone());
        let capture = Capture {
            target,
            machine: entry.machine.clone(),
            root: entry.row["path"].as_str().unwrap().into(),
            common_dir: entry.common_dir.clone(),
            owner_id: entry.expected_owner_id.clone(),
        };
        let id = || uuid::Uuid::new_v4().to_string();
        let original = capture.invocation(&id()).unwrap().stdin.unwrap();
        let input = Input {
            request_id: id(),
            confirmation_id: id(),
            source: stop::Source::Service { epoch: id() },
            manifest: Manifest {
                abi: ABI.into(),
                id: id(),
                capture,
                configuration: "a".repeat(64),
                boot: id(),
                checkout: DirectoryId {
                    device: 1,
                    inode: 2,
                    mount: "private-fixture".into(),
                },
                issued: 1000,
                expires: 1000 + MANIFEST_SECONDS,
                programs: vec![],
            },
            original: String::from_utf8(original).unwrap(),
            archive_id: id(),
            stop: stop::Kind::Normal,
        };
        assert!(input.valid());
        job.archive = Some(ArchiveIdentity {
            version: 1,
            archive_id: input.archive_id.clone(),
            name: input.manifest.capture.target.name.clone(),
            run_id: input.manifest.capture.target.run_id.clone(),
            conversation_id: input.manifest.capture.target.conversation_id.clone(),
            owner_id: input.manifest.capture.owner_id.clone(),
            launch_owned: true,
        });
        job.unconfirmed_stop.as_mut().unwrap().original = Some(Box::new(input.clone()));
        input
    }

    fn folder(machine: &str, path: &str, common: &str, owner: &str) -> Result<Value, String> {
        assert_eq!(machine, "11111111-1111-4111-8111-111111111111");
        assert_eq!(path, "/home/example/repo-work-1");
        assert_eq!(common, "/home/example/repo/.git");
        assert!(zerus_client_core::creation::canonical_uuid(owner));
        Ok(json!({"path": path, "common_dir": common, "review": {
            "unreadable_count": 6
        }}))
    }

    fn unconfirmed_and_kept(inner: &Inner, has_request: bool) {
        let model = lock(&inner.model);
        let view = model.view();
        let row = &view.rows[0];
        assert_eq!(row.status, "kept");
        assert_eq!(row.reason, "The program stop is unconfirmed.");
        assert!(!row.can_check && !row.can_force && !row.can_remove_anyway && !row.ready);
        assert!(model.jobs[0].lookup_only && !model.jobs[0].manual_review);
        let availability = row.program_stop_check.as_ref().unwrap();
        assert!(availability.can_check);
        assert_eq!(availability.has_request, has_request);
    }

    #[test]
    fn absent_request_checks_only_the_recorded_folder_once() {
        let inner = unconfirmed();
        unconfirmed_and_kept(&inner, false);
        let checked = inspect(
            &inner,
            "job",
            true,
            |_| panic!("no original request"),
            folder,
        )
        .unwrap();
        assert_eq!(checked["program_stop_result"], "request_unavailable");
        assert_eq!(checked["review"]["unreadable_count"], 6);
        unconfirmed_and_kept(&inner, false);
    }

    fn reply(value: Value) -> Exit {
        Exit::Code {
            code: 0,
            stdout: serde_json::to_vec(&value).unwrap(),
            stderr: String::new(),
        }
    }

    #[test]
    fn exact_completed_receipt_is_display_only() {
        let inner = unconfirmed();
        let expected = original(&inner);
        unconfirmed_and_kept(&inner, true);
        let checked = inspect(
            &inner,
            "job",
            true,
            |original| {
                assert_eq!(original, &expected);
                status_with(original, |invocation| {
                    assert_eq!(
                        invocation.args,
                        ["worktrees", "program-stop-status", "--json"]
                    );
                    assert_eq!(invocation.timeout, Duration::from_secs(8));
                    assert!(invocation.request_id.is_none());
                    assert_eq!(
                        Input::decode(invocation.stdin.as_deref().unwrap()).unwrap(),
                        expected
                    );
                    reply(json!({"found":true, "report":Report {
                        request_id: original.request_id.clone(), binding:original.binding(),
                        state:State::Complete, members:vec![],
                    }}))
                })
            },
            folder,
        )
        .unwrap();
        assert_eq!(checked["program_stop_result"], "complete");
        unconfirmed_and_kept(&inner, true);
        let passive = inspect(
            &inner,
            "job",
            false,
            |_| panic!("opening a row does not query the receipt"),
            folder,
        )
        .unwrap();
        assert!(passive["program_stop_result"].is_null());
    }

    #[test]
    fn missing_unknown_and_mismatched_receipts_remain_distinct() {
        let inner = unconfirmed();
        let input = original(&inner);
        let valid = Report {
            request_id: input.request_id.clone(),
            binding: input.binding(),
            state: State::Unknown,
            members: vec![],
        };
        assert_eq!(
            status_with(&input, |_| reply(json!({"found":false}))),
            Recorded::ReceiptMissing
        );
        assert_eq!(
            status_with(&input, |_| reply(json!({"found":true,"report":valid}))),
            Recorded::Unconfirmed
        );
        let invalid = Report {
            binding: "b".repeat(64),
            ..valid
        };
        assert_eq!(
            status_with(&input, |_| reply(json!({"found":true,"report":invalid}))),
            Recorded::StatusUnavailable
        );
    }

    #[test]
    fn busy_unknown_moved_or_dismissed_rows_cannot_publish_a_stop_check() {
        let inner = unconfirmed();
        assert!(inspect(
            &inner,
            "different-row",
            true,
            |_| unreachable!(),
            |_, _, _, _| unreachable!()
        )
        .is_err());
        {
            let mut model = lock(&inner.model);
            model.jobs[0].busy = true;
            assert!(check_entry(&model, &model.jobs[0]).is_err());
            model.jobs[0].busy = false;
            model.jobs[0].cleanup.entries[0].status = Some(Status::Unconfirmed);
            assert!(check_entry(&model, &model.jobs[0]).is_err());
            model.jobs[0].cleanup.entries[0].status = Some(Status::Kept);
        }
        let checked = inspect(
            &inner,
            "job",
            true,
            |_| unreachable!(),
            |m, p, c, o| {
                lock(&inner.model).jobs[0].dismissed = true;
                folder(m, p, c, o)
            },
        );
        assert!(checked.is_err());
    }

    #[test]
    fn a_different_archive_cannot_reuse_an_original_stop() {
        let inner = unconfirmed();
        let _ = original(&inner);
        lock(&inner.model).jobs[0]
            .archive
            .as_mut()
            .unwrap()
            .archive_id = uuid::Uuid::new_v4().to_string();
        assert!(inspect(
            &inner,
            "job",
            true,
            |_| unreachable!(),
            |_, _, _, _| unreachable!()
        )
        .is_err());
    }
}
