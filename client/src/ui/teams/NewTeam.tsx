import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { tr } from "../../i18n";
import { selected } from "../../store/ui";
import { machineCards } from "../../store/machines";
import {
  teams, newRequestId, type TeamCall, type TeamCatalog, type TeamCatalogPipeline, type TeamCatalogTemplate,
  type TeamProjectFacts,
} from "../../store/teams";
import { TextInput } from "../lib/TextField";
import "./teams.css";

// New team (M1): a lazy dialog from a catalog template. It shows the roster,
// the pipelines and the settings the person confirms, and it creates nothing
// before Create. The project folder must be a Git folder (checked through the
// core's read) and the base commit comes from that check.

/** A URL-safe template id from a name. */
export function slug(text: string): string {
  return text.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 60) || "team";
}

/** The settings a person confirms, as lines of the template's own values. */
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
  return catalog.pipelines
    .filter((pipeline) => pipeline.id === template.default_pipeline || pipeline.implementer)
    .map((pipeline) => {
      const stages = pipeline.stages.map((stage) => `${tr(stage.kind)}: ${stage.role}`).join(" → ");
      return `${pipeline.name || pipeline.id}${stages ? ` — ${stages}` : ""}`;
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
export function NewTeamForm(props: NewTeamProps) {
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
  const template = createMemo(() => props.catalog?.templates.find((candidate) => candidate.id === templateId()));
  const proposed = createMemo(() => branch() || `zerus/team/${slug(name() || template()?.name || "team")}`);
  const ready = createMemo(() => !!template() && !!name().trim() && !!project().trim() && !!facts()?.git && !props.busy);

  const check = async () => {
    setChecking(true); setError("");
    try { setFacts(await props.onCheckFolder(folder())); }
    catch (failure) { setFacts(undefined); setError(String(failure)); }
    finally { setChecking(false); }
  };
  const create = () => {
    const chosen = template();
    const checked = facts();
    if (!chosen || !checked?.git) return;
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
    <div class="nt-body">
      <Show when={props.catalogError}><div class="tm-notice tm-bad" role="alert">{props.catalogError}</div></Show>
      <div class="nt-row">
        <label>{tr("Template")}
          <select class="select" value={templateId()} onChange={(event) => setTemplateId(event.currentTarget.value)}>
            <For each={props.catalog?.templates ?? []}>{(candidate) => <option value={candidate.id}>{candidate.name || candidate.id}</option>}</For>
          </select>
        </label>
        <label>{tr("Team name")}
          <TextInput class="input" value={name()} onInput={(event) => setName(event.currentTarget.value)} placeholder={tr("Fixes")} historyKey="team-name" />
        </label>
      </div>
      <div class="nt-row">
        <label>{tr("Goal")}
          <TextInput class="input" value={goal()} onInput={(event) => setGoal(event.currentTarget.value)} placeholder={tr("What should the team work on?")} historyKey="team-goal" />
        </label>
        <label>{tr("Project")}
          <TextInput class="input" value={project()} onInput={(event) => setProject(event.currentTarget.value)} historyKey="team-project" />
        </label>
      </div>
      <Show when={template()}>
        {(chosen) => (
          <>
            <p class="nt-hint">{chosen().description}</p>
            <div class="nt-section">
              <h3>{tr("Roster")}</h3>
              <ul class="nt-list"><For each={rosterLines(props.catalog!, chosen())}>{(line) => <li>{line}</li>}</For></ul>
            </div>
            <div class="nt-section">
              <h3>{tr("Pipelines")}</h3>
              <ul class="nt-list"><For each={pipelineLines(props.catalog!, chosen())}>{(line) => <li>{line}</li>}</For></ul>
            </div>
            <div class="nt-section">
              <h3>{tr("Settings")}</h3>
              <dl class="nt-settings"><For each={settingLines(chosen())}>{([label, value]) => <><dt>{label}</dt><dd>{value}</dd></>}</For></dl>
            </div>
          </>
        )}
      </Show>
      <div class="nt-section">
        <h3>{tr("Project folder")}</h3>
        <div class="nt-row">
          <label>{tr("Folder")}
            <TextInput class="input" value={folder()} onInput={(event) => { setFolder(event.currentTarget.value); setFacts(undefined); }} historyKey="team-folder" />
          </label>
          <button type="button" class="btn small" disabled={checking() || !folder().trim()} onClick={() => void check()}>{tr("Check folder")}</button>
        </div>
        <Show when={facts()}>
          {(checked) => checked().git
            ? <span class="nt-ok" data-folder-ok>{tr("A Git folder at %1, base commit %2.", checked().branch || tr("detached"), checked().head.slice(0, 12))}</span>
            : <span class="nt-bad" data-folder-bad>{tr("Not a Git folder: %1", checked().reason)}</span>}
        </Show>
        <label>{tr("Integration branch")}
          <TextInput class="input" value={proposed()} onInput={(event) => setBranch(event.currentTarget.value)} disabled={template()?.settings.integration !== "branch"} historyKey="team-branch" />
        </label>
      </div>
      <Show when={error()}><div class="tm-notice tm-bad" role="alert" data-new-team-error>{error()}</div></Show>
      <Show when={saved()}><div class="tm-notice" role="status">{saved()}</div></Show>
      <div class="nt-foot">
        <button type="button" class="btn small ghost" disabled={!template() || saving()} onClick={() => void save()}>{tr("Save as template")}</button>
        <span class="grow" />
        <button type="button" class="btn" onClick={props.onClose}>{tr("Cancel")}</button>
        <button type="button" class="btn primary" disabled={!ready()} onClick={create}>{tr("Create team")}</button>
      </div>
    </div>
  );
}

/** The dialog the rail and the command table open; it loads the catalog once. */
export function NewTeamDialog() {
  let dialog: HTMLDialogElement | undefined;
  const open = createMemo(() => teams.newTeamOpen());
  onMount(() => {
    if (open()) dialog?.showModal();
  });
  onCleanup(() => { if (dialog?.open) dialog.close(); });
  return (
    <Show when={open()}>
      <dialog ref={dialog} class="dlg nt-dialog" aria-labelledby="new-team-title" open
        onCancel={(event) => { event.preventDefault(); teams.closeNewTeam(); }}>
        <div class="dlg-head"><h2 id="new-team-title">{tr("New team")}</h2></div>
        <div class="dlg-body">
          <NewTeamForm
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
              void teams.run(call).then((outcome) => { if (outcome.outcome === "done") teams.closeNewTeam(); });
            }}
            onSaveTemplate={(template) => teams.saveTemplate(template)}
            onClose={() => teams.closeNewTeam()}
          />
        </div>
      </dialog>
    </Show>
  );
}
