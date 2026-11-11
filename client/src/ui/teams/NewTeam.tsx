import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { JSX } from "solid-js";
import { tr } from "../../i18n";
import { selected } from "../../store/ui";
import { machineCards } from "../../store/machines";
import {
  teams, newRequestId, type TeamCall, type TeamCatalog, type TeamCatalogTemplate, type TeamProjectFacts,
} from "../../store/teams";
import { NewTeamFooter } from "./NewTeamFooter";
import { NewTeamHowTo, NewTeamIntroSteps } from "./NewTeamGuide";
import { NewTeamPage } from "./NewTeamPage";
import { TextInput } from "../lib/TextField";
import "../styles/newteam.css";

// New team (Qt's NewTeamWindow, the fork's form adapted to M1): a dialog from a
// catalog template. It shows the roster, the pipelines and the settings the
// person confirms; the folder is checked as a Git folder before anything can be
// created, and Create team is the only thing that creates anything. The fork's
// drafts, plan modes, working modes, accounts and member editing are M2 and are
// not here.

/** A URL-safe template id from a name. */
export function slug(text: string): string {
  return text.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 60) || "team";
}

/** The settings a person confirms, as the template's own values. */
export function settingLines(template: TeamCatalogTemplate): [string, string][] {
  const settings = template.settings;
  return [
    [tr("Integration"), settings.integration === "branch" ? tr("an integration branch") : tr("no integration branch")],
    [tr("Plan approval"), settings.plan_approval === "review" ? tr("the person approves the plan") : tr("automatic")],
    [tr("Acceptance"), settings.phase_acceptance === "lead" ? tr("the lead") : settings.phase_acceptance === "person" ? tr("the person") : tr("the pipeline")],
    [tr("Queue"), settings.queue ? tr("the team takes tasks as they come") : tr("planned phases only")],
    [tr("Routing"), settings.routing === "direct" ? tr("straight to a free member") : tr("the lead routes first")],
    [tr("Land stage"), settings.land_allowed ? tr("allowed") : tr("not allowed")],
  ];
}

export function rosterLines(catalog: TeamCatalog, template: TeamCatalogTemplate): string[] {
  return template.roster.map((seat) => {
    const role = catalog.roles.find((candidate) => candidate.id === seat.role);
    const name = role?.name ? tr(role.name) : seat.role;
    return tr("%1 × %2%3", name, seat.count, seat.optional ? tr(" (optional)") : "");
  });
}

export function pipelineLines(catalog: TeamCatalog, template: TeamCatalogTemplate): string[] {
  return catalog.pipelines.map((pipeline) => {
    const stages = pipeline.stages.map((stage) => `${tr(stage.kind)}: ${stage.role}`).join(" → ");
    return `${pipeline.name || pipeline.id}${stages ? ` — ${stages}` : ""}${pipeline.id === template.default_pipeline ? "" : ""}`;
  });
}

export interface NewTeamInput {
  /** The new team's id, made here once. */
  team: string;
  template: string;
  project: string;
  name: string;
  goal: string;
  home: string;
  integration_branch: string;
  base_commit: string;
  integration: string;
}

export interface NewTeamProps {
  catalog?: TeamCatalog;
  catalogError?: string;
  busy: boolean;
  machineId: string;
  defaultProject: string;
  defaultFolder: string;
  onCheckFolder: (path: string) => Promise<TeamProjectFacts>;
  onCreate: (input: NewTeamInput) => void;
  onSaveTemplate: (template: TeamCatalogTemplate) => Promise<void>;
  onClose: () => void;
}

/** The form alone, from a catalog: the render tests pass their own. */
export function NewTeam(props: NewTeamProps): JSX.Element {
  const [templateId, setTemplateId] = createSignal(props.catalog?.templates[0]?.id ?? "");
  const [name, setName] = createSignal("");
  const [goal, setGoal] = createSignal("");
  const [project, setProject] = createSignal(props.defaultProject);
  const [folder, setFolder] = createSignal(props.defaultFolder);
  const [branch, setBranch] = createSignal("");
  const [facts, setFacts] = createSignal<TeamProjectFacts>();
  const [checking, setChecking] = createSignal(false);
  const [error, setError] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [saved, setSaved] = createSignal("");
  const [howto, setHowto] = createSignal(false);
  const template = createMemo(() => props.catalog?.templates.find((candidate) => candidate.id === templateId()));
  const proposed = createMemo(() => branch() || `zerus/team/${slug(name() || template()?.name || "team")}`);
  const createWhy = createMemo(() => {
    if (!template()) return tr("Choose a template.");
    if (!name().trim()) return tr("Name the team.");
    if (!project().trim()) return tr("Name the project.");
    if (!facts()?.git) return tr("Check the project folder: it must be a Git folder.");
    if (props.busy) return tr("Wait for the team to be created.");
    return "";
  });

  const check = async () => {
    setChecking(true); setError("");
    try { setFacts(await props.onCheckFolder(folder())); }
    catch (failure) { setFacts(undefined); setError(String(failure)); }
    finally { setChecking(false); }
  };
  const create = () => {
    const chosen = template();
    const checked = facts();
    if (!chosen || !checked?.git || createWhy()) return;
    props.onCreate({
      team: newRequestId(), template: chosen.id, project: project().trim(), name: name().trim(), goal: goal().trim(),
      home: props.machineId, integration_branch: proposed(), base_commit: checked.head, integration: chosen.settings.integration,
    });
  };
  const save = async () => {
    const chosen = template();
    if (!chosen) return;
    setSaving(true); setError("");
    try {
      await props.onSaveTemplate({ ...chosen, id: slug(`${chosen.id}-${name() || chosen.name}`), name: name().trim() || chosen.name });
      setSaved(tr("Template saved."));
    } catch (failure) { setError(String(failure)); }
    finally { setSaving(false); }
  };

  return (
    <div class="nt-body" data-new-team-form>
      <Show when={props.catalogError}><div class="nt-error" role="alert">{props.catalogError}</div></Show>
      <section class="nt-templates" aria-label={tr("Template")}>
        <div class="nt-inline"><b>{tr("Template")}</b></div>
        <div class="nt-template-list" role="listbox" aria-label={tr("Template")}>
          <For each={props.catalog?.templates ?? []}>
            {(candidate) => (
              <button type="button" class="nt-template-row" role="option" aria-selected={templateId() === candidate.id}
                data-template={candidate.id} title={candidate.description || candidate.name} onClick={() => setTemplateId(candidate.id)}>
                <span class="nt-template-name">{candidate.name || candidate.id}</span>
                <span class="nt-hint">{candidate.description}</span>
              </button>
            )}
          </For>
        </div>
        <Show when={template()}>{(chosen) => <p class="nt-hint nt-template-detail">{chosen().description}</p>}</Show>
      </section>
      <div class="nt-pair">
        <label class="nt-field">{tr("Team name")}
          <TextInput class="field" value={name()} onInput={(event) => setName(event.currentTarget.value)} placeholder={tr("Fixes")} historyKey="team-name" />
        </label>
        <label class="nt-field">{tr("Goal")}
          <TextInput class="field" value={goal()} onInput={(event) => setGoal(event.currentTarget.value)} placeholder={tr("What should the team work on?")} historyKey="team-goal" />
        </label>
      </div>
      <div class="nt-pair">
        <label class="nt-field">{tr("Project")}
          <TextInput class="field" value={project()} onInput={(event) => setProject(event.currentTarget.value)} historyKey="team-project" />
        </label>
      </div>
      <Show when={template()}>
        {(chosen) => (
          <div class="nt-pair">
            <section class="nt-field" aria-label={tr("Roster")}>
              <b>{tr("Roster")}</b>
              <ul class="nt-list"><For each={rosterLines(props.catalog!, chosen())}>{(line) => <li>{line}</li>}</For></ul>
            </section>
            <section class="nt-field" aria-label={tr("Pipelines")}>
              <b>{tr("Pipelines")}</b>
              <ul class="nt-list"><For each={pipelineLines(props.catalog!, chosen())}>{(line) => <li>{line}</li>}</For></ul>
            </section>
          </div>
        )}
      </Show>
      <Show when={template()}>
        {(chosen) => (
          <section class="nt-field" aria-label={tr("Settings")}>
            <b>{tr("Settings")}</b>
            <dl class="nt-caps"><For each={settingLines(chosen())}>{([label, value]) => <><dt>{label}</dt><dd>{value}</dd></>}</For></dl>
          </section>
        )}
      </Show>
      <section class="nt-field" aria-label={tr("Project folder")}>
        <b>{tr("Project folder")}</b>
        <div class="nt-inline">
          <TextInput class="field grow" value={folder()} onInput={(event) => { setFolder(event.currentTarget.value); setFacts(undefined); }} historyKey="team-folder" />
          <button type="button" class="btn small" disabled={checking() || !folder().trim()} onClick={() => void check()}>{tr("Check folder")}</button>
        </div>
        <Show when={facts()}>
          {(checked) => checked().git
            ? <span class="nt-ok" data-folder-ok>{tr("A Git folder at %1, base commit %2.", checked().branch || tr("detached"), checked().head.slice(0, 12))}</span>
            : <span class="nt-foot-bad" data-folder-bad>{tr("Not a Git folder: %1", checked().reason)}</span>}
        </Show>
        <label class="nt-field">{tr("Integration branch")}
          <TextInput class="field" value={proposed()} onInput={(event) => setBranch(event.currentTarget.value)}
            disabled={template()?.settings.integration !== "branch"} historyKey="team-branch" />
        </label>
      </section>
      <Show when={error()}><div class="nt-error" role="alert" data-new-team-error>{error()}</div></Show>
      <Show when={saved()}><div class="nt-note" role="status">{saved()}</div></Show>
      <NewTeamIntroSteps />
      <NewTeamHowTo open={howto()} onToggle={setHowto} />
      <NewTeamFooter busy={props.busy} createWhy={createWhy()} error={error()} canSave={!!template()} saving={saving()}
        onSaveTemplate={() => void save()} onCancel={props.onClose} onCreate={create} />
    </div>
  );
}

/** The dialog the rail and the command table open; the catalog is read on open. */
export function NewTeamDialog() {
  let dialog: HTMLDialogElement | undefined;
  const open = createMemo(() => teams.newTeamOpen());
  onMount(() => { if (open()) dialog?.showModal(); });
  onCleanup(() => { if (dialog?.open) dialog.close(); });
  const close = () => teams.closeNewTeam();
  return (
    <Show when={open()}>
      <dialog ref={dialog} class="dlg newteam-page-dialog" aria-labelledby="new-team-title" open
        onCancel={(event) => { event.preventDefault(); close(); }}>
        <NewTeamPage onClose={close}>
          <NewTeam
            catalog={teams.catalog()}
            catalogError={teams.catalogError()}
            busy={teams.busy()}
            machineId={machineCards[""]?.machine_id ?? ""}
            defaultProject={selected()?.project ?? ""}
            defaultFolder={selected()?.gitRoot || selected()?.cwdPath || ""}
            onCheckFolder={(path) => teams.project(path)}
            onCreate={(input) => {
              const call: TeamCall = {
                team: input.team, team_name: input.name, request_id: newRequestId(),
                action: {
                  kind: "create", template: input.template, project: input.project, name: input.name, goal: input.goal,
                  home: input.home, integration_branch: input.integration_branch, base_commit: input.base_commit,
                  integration: input.integration,
                },
              };
              void teams.run(call).then((outcome) => { if (outcome.outcome === "done") close(); });
            }}
            onSaveTemplate={(template) => teams.saveTemplate(template)}
            onClose={close}
          />
        </NewTeamPage>
      </dialog>
    </Show>
  );
}
