import { describe, it, expect, vi } from "vitest";
import { createAccountManagement } from "./accountManagement";
import type { AccountEditKind, AccountEditView } from "../bridge";

const origin = {} as HTMLElement;
function setup() {
  const open=vi.fn(async (_account:string,_profile:string,kind:AccountEditKind):Promise<AccountEditView>=>({handle:"native-snapshot",kind,label:"Example",machine:"studio",provider:"codex"}));
  const apply=vi.fn(async (_handle:string,_label:string)=>({notices:["Account name updated."],errors:[] as string[]}));
  const close=vi.fn(async (_handle:string)=>{}), notice=vi.fn();
  return { open,apply,close,notice, state:createAccountManagement({open,apply,close,notice}) };
}
describe("captured account catalog actions",()=>{
  it("applies the captured handle once and never retargets by the later selection",async()=>{
    const t=setup();await t.state.open("rename","account-group","studio-profile",origin);
    expect(t.open).toHaveBeenCalledWith("account-group","studio-profile","rename");
    expect(t.state.prompt()?.origin).toBe(origin);
    await Promise.all([t.state.confirm("New label"),t.state.confirm("New label")]);
    expect(t.apply).toHaveBeenCalledExactlyOnceWith("native-snapshot","New label");
    expect(t.state.prompt()).toBeUndefined();expect(t.notice).toHaveBeenCalledWith(false,"Account name updated.");
    expect(t.state.notice()).toBe("Account name updated.");t.state.clearNotice();expect(t.state.notice()).toBe("");
  });
  it("cancels removal and releases its native snapshot without applying",async()=>{
    const t=setup();await t.state.open("remove","group","peer-profile",origin);t.state.close();
    expect(t.apply).not.toHaveBeenCalled();expect(t.close).toHaveBeenCalledWith("native-snapshot");
  });
  it("closes an unchanged rename without a catalog write",async()=>{
    const t=setup();await t.state.open("rename","group","profile",origin);await t.state.confirm(" Example ");
    expect(t.apply).not.toHaveBeenCalled();expect(t.close).toHaveBeenCalledWith("native-snapshot");
  });
  it("makes the chosen profile default directly with one captured request",async()=>{
    const t=setup();await t.state.open("default","group","peer-profile",origin);
    expect(t.state.prompt()).toBeUndefined();expect(t.apply).toHaveBeenCalledExactlyOnceWith("native-snapshot","");
  });
  it("reports uncertain or partial errors without replaying",async()=>{
    const t=setup();t.apply.mockRejectedValueOnce(new Error("Delivery uncertain"));
    await t.state.open("remove","group","profile",origin);await t.state.confirm();await t.state.confirm();
    expect(t.apply).toHaveBeenCalledTimes(1);expect(t.notice).toHaveBeenCalledWith(true,"Error: Delivery uncertain");expect(t.state.busy()).toBe(false);
    t.apply.mockResolvedValueOnce({notices:["Account name updated."],errors:["studio: Accounts changed"]});
    await t.state.open("rename","group","profile",origin);await t.state.confirm("Another");
    expect(t.notice).toHaveBeenCalledWith(true,"studio: Accounts changed");
  });
  it("discards a late open response after its screen closes",async()=>{
    const t=setup();let resolve!:(v:AccountEditView)=>void;
    t.open.mockImplementationOnce(()=>new Promise(r=>{resolve=r;}));
    const pending=t.state.open("remove","group","profile",origin);t.state.close();
    resolve({handle:"late",kind:"remove",label:"Example",machine:"studio",provider:"codex"});await pending;
    expect(t.state.prompt()).toBeUndefined();expect(t.close).toHaveBeenCalledWith("late");expect(t.apply).not.toHaveBeenCalled();
  });
});
