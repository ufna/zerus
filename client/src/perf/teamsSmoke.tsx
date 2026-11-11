// Keep the shared layer order first, just as main.tsx does.
import "../ui/styles/base.css";
import "../ui/styles/tasks.css";
import { For, Show, createComponent, onMount, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam } from "../ui/teams/NewTeam";
import { NewTeamPage } from "../ui/teams/NewTeamPage";
import { TeamUncertain, TeamWorkspace } from "../ui/teams/TeamTab";
import { MemberCard } from "../ui/teams/MemberCard";
import { catalog, teamView } from "../ui/teams/fixtures";
import type { TeamView } from "../store/teams";
import { applySkin, skins } from "../skin/runtime";

// Fixture-only views: no store follows, IPC or desktop application. Query:
// ?skin=zerus-dark|zerus-light|fjord|paper&screen=team|new|states&narrow=1
// &detail=1 selects the review card; &ready=1 fills the name and checks the
// synthetic folder. Without screen, the page shows every fixture.
//   ./node_modules/.bin/vite --port 5181 --strictPort --host 127.0.0.1
//   http://127.0.0.1:5181/scripts/teams-smoke.html
const params = new URLSearchParams(location.search);
const skin = skins.find((skin) => skin.id === params.get("skin")) ?? skins[0];
applySkin(skin);
document.documentElement.dataset.motion = "off";

const members = [
  teamView.members[0],
  teamView.members[1],
  { ...teamView.members[3], state: "paused" },
  { ...teamView.members[2], id: "tester", role: "tester", state: "not_created" },
  { ...teamView.members[2], id: "old", state: "removed" },
];
const full: TeamView = { ...teamView, members, next: "fixture-next-page", truncated: true };
const empty: TeamView = {
  ...teamView, members: members.slice(0, 2),
  groups: teamView.groups.map((group) => ({ ...group, cards: [] })),
  bar: { ...teamView.bar, counts: { ...teamView.bar.counts, nodes: { waiting: 0, pending: 0, in_progress: 0, reported: 0, accepted: 0, cancelled: 0 }, inbox: 0, cards: 0 } },
};
const previewCatalog = { ...catalog, templates: [...catalog.templates, {
  ...catalog.templates[0], id: "small", name: "Small", description: "One implementer and an independent checker.",
  roster: catalog.templates[0].roster.map((seat) => ({ ...seat, count: 1 })),
}] };

function Section(props: { title: string; note?: string; children: JSX.Element; form?: boolean }) {
  return <section class="smoke-section" data-smoke={props.title} classList={{ "smoke-form": props.form }}>
    <h2>{props.title}</h2>
    <Show when={props.note}><p class="dim">{props.note}</p></Show>
    <div class="smoke-frame">{props.children}</div>
  </section>;
}

function FormPreview() {
  let dialog: HTMLDialogElement | undefined;
  onMount(() => {
    dialog?.showModal();
    if (params.has("ready")) queueMicrotask(() => {
      const field = document.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
      field.value = "Fixes"; field.dispatchEvent(new Event("input", { bubbles: true }));
      document.querySelector<HTMLButtonElement>("[data-check-folder]")!.click();
    });
  });
  const form = () => <NewTeamPage onClose={() => {}}>{createComponent(NewTeam, {
    catalog: previewCatalog, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa",
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: () => {}, onSaveTemplate: async () => {}, onClose: () => {},
  })}</NewTeamPage>;
  return params.get("screen") === "new"
    ? <dialog ref={dialog} class="dlg newteam-page-dialog" aria-labelledby="new-team-title">{form()}</dialog>
    : form();
}

function App() {
  const show = (screen: string) => !params.has("screen") || params.get("screen") === screen;
  onMount(() => {
    if (params.has("detail")) queueMicrotask(() => {
      const card = document.querySelector<HTMLButtonElement>('[data-group="reported"] .team-task-card');
      card?.click();
      if (window.innerWidth <= 900) card?.scrollIntoView({ block: "nearest", inline: "nearest" });
      window.scrollTo(0, 0);
    });
  });
  return <div class="smoke" classList={{ "smoke-narrow": params.has("narrow") }}>
    <header class="smoke-head"><h1>Teams screens (M1)</h1><nav aria-label="Preview skins"><For each={skins}>{(choice) => {
      const query = new URLSearchParams(params); query.set("skin", choice.id);
      return <a class="btn small" aria-current={choice.id === skin.id ? "page" : undefined} href={`?${query}`}>{choice.name}</a>;
    }}</For></nav></header>
    <Show when={show("team")}>
      <Section title="Team tab: a full board with stages" note="Select a card for its facts and decisions. More uses a fixture cursor.">
        <TeamWorkspace view={full} busy={false} failure={null} onAction={() => {}} onPage={() => {}} onAddTask={() => {}} />
      </Section>
      <Section title="An uncertain action"><TeamUncertain detail="The action may have been applied. Check before doing anything else." busy={false} onCheck={() => {}} onDismiss={() => {}} /></Section>
      <Section title="Team tab: an empty queue and a read failure">
        <TeamWorkspace view={empty} busy={false} failure={{ code: "read_failed", message: "The computer is offline." }} onAction={() => {}} onPage={() => {}} onAddTask={() => {}} />
      </Section>
    </Show>
    <Show when={show("states")}>
      <Section title="Members in each state" note="active, paused, not created and removed, with the lead marked.">
        <div class="smoke-members"><For each={members}>{(member) => <MemberCard member={member} busy={false} onAction={() => {}} />}</For></div>
      </Section>
      <Section title="Teams off"><p class="smoke-off dim">With the preview off, the rail's New team button, the team.new command and the Team tab have no entry point.</p></Section>
    </Show>
    <Show when={show("new")}>
      <Section title="New team" form note="The real dialog frame. Use ready=1 to check the synthetic Git folder."><FormPreview /></Section>
    </Show>
  </div>;
}

render(() => <App />, document.getElementById("root")!);
