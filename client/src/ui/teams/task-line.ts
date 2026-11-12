import { tr } from "../../i18n";
import type { StageView } from "./types";

/** A stage's own worker (`assignee`) and the implementer of the task it checks are separate fields. */
export function stageLine(stage: StageView): string {
  return tr("%1: %2, checks the work of %3", tr(stage.role), stage.assignee || tr("unassigned"), stage.implementer || tr("nobody"));
}
