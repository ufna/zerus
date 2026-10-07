<h1 align="center">
  <img src="docs/assets/zerus.svg" alt="" width="64" valign="middle" /> Zerus
</h1>

<p align="center">
  <strong>One ADE for dozens or hundreds of agents across your machines.</strong><br />
  Projects, sessions, messages and notifications in one place. Your agents keep working when you close the window.
</p>

<p align="center">
  <a href="docs/installation.md">Install</a> &nbsp; | &nbsp;
  <a href="#features">Features</a> &nbsp; | &nbsp;
  <a href="docs/reference.md">Reference</a><br />
  <sub>Linux and macOS &nbsp; | &nbsp; tmux + SSH &nbsp; | &nbsp; P2P &nbsp; | &nbsp; MIT</sub>
</p>

<p align="center">
  <a href="https://code.claude.com/"><kbd><img src="https://www.google.com/s2/favicons?domain=claude.ai&amp;sz=64" alt="" width="16" valign="middle" /> Claude Code</kbd></a> &nbsp;
  <a href="https://github.com/openai/codex"><kbd><img src="https://www.google.com/s2/favicons?domain=openai.com&amp;sz=64" alt="" width="16" valign="middle" /> Codex</kbd></a> &nbsp;
  <a href="https://www.kimi.com/code/"><kbd><img src="https://www.google.com/s2/favicons?domain=kimi.com&amp;sz=64" alt="" width="16" valign="middle" /> Kimi Code</kbd></a> &nbsp;
  <a href="https://deepseek-harness.github.io/deepseek-harness/"><kbd><img src="https://www.google.com/s2/favicons?domain=deepseek.com&amp;sz=64" alt="" width="16" valign="middle" /> DeepSeek Harness</kbd></a>
</p>

Zerus is an Agent Development Environment built around persistent sessions. Work
on several projects, spread agents across your laptop and servers, follow their
progress and respond wherever your attention is needed.

**Claude Code, Codex, Kimi Code and DeepSeek Harness** have close native
integrations: Activity, messages, questions, approvals and session controls.
Available actions depend on the agent and its version.
You can run **any terminal command** through `hgs`; full ADE features need an
adapter. Missing your agent or a feature?
[Send a PR](https://github.com/ufna/zerus/pulls) or
[let us know](https://github.com/ufna/zerus/issues).

<p align="center">
  <img src="docs/assets/readme/workspace.webp" alt="Zerus workspace with multiple agents, Activity, subagents and tasks in one window" width="1100" />
</p>

## Features

<table>
  <tr>
    <td width="42%" valign="middle">
      <h3>Your whole fleet at a glance</h3>
      <p>See machines, running sessions and requests for attention in one overview. Search, filters and project groups help you work with dozens or hundreds of agents. Machine load and account limits are right alongside them.</p>
    </td>
    <td width="58%">
      <a href="docs/assets/readme/fleet.webp"><img src="docs/assets/readme/fleet.webp" alt="Overview of 112 synthetic sessions on four machines, with activity and resource use" width="640" /></a>
    </td>
  </tr>
  <tr>
    <td valign="middle">
      <h3>Notifications that lead to the right session</h3>
      <p>Know when an agent finishes, asks a question, requests approval or hits an error. A click takes you to that session's Activity, including remote sessions. Read replies, send messages and images, and follow subagents and tasks alongside the conversation.</p>
    </td>
    <td>
      <a href="docs/assets/readme/attention.webp"><img src="docs/assets/readme/attention.webp" alt="A remote DeepSeek Harness session waiting for a review-scope choice in Activity" width="640" /></a>
    </td>
  </tr>
  <tr>
    <td valign="middle">
      <h3>Projects and shared settings over P2P</h3>
      <p>A project brings together folders and sessions from different machines. The project catalog and shared recovery rules sync over SSH, without an external cloud. Offline machines catch up when they reconnect. SSH settings, secrets and workspace appearance stay local.</p>
    </td>
    <td>
      <a href="docs/assets/readme/projects.webp"><img src="docs/assets/readme/projects.webp" alt="The Orbit project connects folders on a workstation, a MacBook and two servers" width="640" /></a>
    </td>
  </tr>
</table>

### Persistent sessions, your choice of client

Terminal agents run in **tmux**. Closing Zerus, losing SSH or putting your laptop
to sleep leaves sessions on a running host intact. Attach from the built-in
Terminal, a regular terminal over SSH or another client, even one without Zerus.
Supported agents also have pause/resume with an exact conversation binding.

**DeepSeek Harness** uses the official native engine and an embedded
**Native UI** alongside Activity. Its host also keeps working after you close
the window. [Integration details](docs/reference.md#deepseek).

## Quick start

[Install Zerus and the CLI](docs/installation.md), then use **New session** to
choose an agent, machine, project and folder. Ordinary folders without Git work
too. Add other machines through SSH in **Machines**.

The same sessions are available from your terminal:

```sh
hgs codex /workspace/orbit -n api       # local agent in tmux
hgs @build-01 claude /workspace/orbit   # agent on another machine
hgs ls                                # sessions here and on connected machines
hgs a codex/orbit/api                  # attach from any terminal
```

**On the roadmap: a mobile client** to follow agents, receive notifications and
reply from your phone.

[Command and settings reference](docs/reference.md) | [MIT](LICENSE)

<sub>Screenshots show the real Zerus interface with synthetic projects, machines and messages.</sub>
