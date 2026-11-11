import { For, Show, createComponent } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam } from "../ui/teams/NewTeam";
import { TeamUncertain, TeamWorkspace } from "../ui/teams/TeamTab";
import { MemberCard } from "../ui/teams/MemberCard";
import { catalog, teamView } from "../ui/teams/fixtures";
import type { TeamView } from "../store/teams";
import "../ui/styles/teams.css";
import "../ui/styles/newteam.css";

// A smoke page for the Teams screens (M1), for screenshots without the desktop
// application: the Team tab with a full board and with an empty queue, a member
// in each state, an uncertain action and New team from the pinned fixtures.
// It renders the views only (no store, no IPC) and drives no operation.
//   npx vite --port 5179 --strictPort   (in client/)
//   http://127.0.0.1:5179/scripts/teams-smoke.html

const members = [
  { ...teamView.members[0], id: "lead", role: "lead", lead: true, state: "active" },
  { ...teamView.members[1], id: "dev", role: "developer", state: "active" },
  { ...teamView.members[2], id: "rev", role: "reviewer", state: "paused", account: "" },
  { ...teamView.members[2], id: "tst", role: "tester", state: "not_created", account: "", session: null },
  { ...teamView.members[2], id: "old", role: "developer", state: "removed", account: "", session: null },
];

const full: TeamView = { ...teamView, members };
const empty: TeamView = {
  ...teamView,
  members,
  groups: teamView.groups.map((group) => ({ ...group, cards: [] })),
  bar: { ...teamView.bar, counts: { ...teamView.bar.counts, nodes: { waiting: 0, pending: 0, in_progress: 0, reported: 0, accepted: 0, cancelled: 0 }, inbox: 0, cards: 0 } },
};

function Section(props: { title: string; note?: string; children: unknown }) {
  return (
    <section class="smoke-section" data-smoke={props.title}>
      <h2>{props.title}</h2>
      <Show when={props.note}>{(note) => <p class="dim">{note()}</p>}</Show>
      <div class="smoke-frame">{props.children as never}</div>
    </section>
  );
}

function TeamsOff() {
  return (
    <div class="smoke-off" data-teams-off>
      <p class="dim">Teams are off on this computer (or hgs cannot run): the rail's New team button, the team.new command and the Team tab show no entry point anywhere.</p>
    </div>
  );
}

function App() {
  return (
    <div class="smoke">
      <h1>Teams screens (M1)</h1>
      <Section title="Team tab: a full board with stages" note="Two cards, a stage's assignee and the task's implementer apart, a member in each state.">
        {createComponent(TeamWorkspace, { view: full, busy: false, failure: null, onAction: () => {}, onPage: () => {}, onAddTask: () => {} })}
      </Section>
      <Section title="Team tab: an empty queue" note="No cards; the queue is its own group, so the board still shows the empty states.">
        {createComponent(TeamWorkspace, { view: empty, busy: false, failure: null, onAction: () => {}, onPage: () => {}, onAddTask: () => {} })}
      </Section>
      <Section title="An uncertain action" note="Only Check and Dismiss; nothing repeats it by itself.">
        {createComponent(TeamUncertain, { detail: "The action may have been applied. Check before doing anything else.", busy: false, onCheck: () => {}, onDismiss: () => {} })}
      </Section>
      <Section title="Members in each state" note="active, paused, not created and removed, with the lead marked.">
        <div class="smoke-members"><For each={members}>{(member) => createComponent(MemberCard, { member, busy: false, onAction: () => {} })}</For></div>
      </Section>
      <Section title="Teams off" note="With the preview off no entry point exists anywhere.">
        {createComponent(TeamsOff, {})}
      </Section>
      <Section title="New team" note="From the pinned bugfix template; Create team waits for the Git folder check.">
        {createComponent(NewTeam, {
          catalog, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa",
          defaultProject: "example-project", defaultFolder: "/workspace/example",
          onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
          onCreate: () => {}, onSaveTemplate: async () => {}, onClose: () => {},
        })}
      </Section>
    </div>
  );
}

const root = document.getElementById("root")!;
render(() => <App />, root);
