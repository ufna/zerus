import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { Composer } from "../../src/ui/sessions/Composer";
import { TaskInput } from "../../src/ui/screens/TaskInput";
import { taskItem } from "../../src/ui/screens/tasks-ui";
import { loadMarkdown } from "../../src/ui/sessions/markdown";
import { TextInput, TextArea } from "../../src/ui/lib/TextField";
import { installTextEditingKeys, replaceText } from "../../src/ui/lib/text-edit";
import { model, open, handBack, sends, seedTask, taskModel, taskSaves } from "./model";

const results: {name:string;pass:boolean;detail?:unknown}[]=[];
const eq=(name:string,actual:unknown,expected:unknown)=>{
 const pass=JSON.stringify(actual)===JSON.stringify(expected);results.push({name,pass,...(pass?{}:{detail:{actual,expected}})});
};
const tick=(ms=0)=>new Promise(r=>setTimeout(r,ms));
const command=(name:string,value?:string)=>document.execCommand(name,false,value);
const mac=/Mac/.test(navigator.platform);
const key=(el:HTMLElement,key:string,extra:KeyboardEventInit={})=>el.dispatchEvent(new KeyboardEvent("keydown",{key,bubbles:true,cancelable:true,...extra,...(mac && extra.ctrlKey?{ctrlKey:false,metaKey:true}:{})}));
installTextEditingKeys(window,mac);
const [dialog,setDialog]=createSignal(""),[epoch,setEpoch]=createSignal(1);
const [number,setNumber]=createSignal(10);
const [change,setChange]=createSignal(""),[ime,setIme]=createSignal("");
const [taskNumber, setTaskNumber] = createSignal(0);
let inputs=0;
render(()=><><Composer/><TextInput id="dialog" value={dialog()} historyKey={epoch()} onInput={e=>{inputs++;setDialog(e.currentTarget.value);}}/>
 <TextInput id="change" value={change()} onChange={e=>setChange(e.currentTarget.value)}/>
 <TextInput id="number" type="number" value={number()} onInput={e=>setNumber(e.currentTarget.valueAsNumber)}/>
 <TextArea id="ime" value={ime()} onInput={e=>setIme(e.currentTarget.value)}/>
 <div id="task-fixture"><TaskInput project="fixture" task={taskItem({ uuid: "task-" + taskNumber(), stage: "active" })} describing={true}
   cancel={() => {}} saved={() => {}} /></div></>,document.getElementById("fixture")!);
const editor=()=>document.querySelector<HTMLTextAreaElement>(".cmp-editor")!;
type EditorKind = "composer" | "task";
const listEditor = (kind: EditorKind) => kind === "composer" ? editor() : document.querySelector<HTMLTextAreaElement>("#task-fixture .cmp-editor")!;
const submitted = (kind: EditorKind) => kind === "composer" ? sends : taskSaves;
const post = (data: unknown) => (window as any).webkit?.messageHandlers.result.postMessage(JSON.stringify(data));
function listSnapshot(kind: EditorKind, stage: string) {
  const el = listEditor(kind);
  post({ stage, value: el.value, draft: kind === "composer" ? model.text() : taskModel.view()?.text,
    start: el.selectionStart, end: el.selectionEnd, submissions: submitted(kind) });
}
async function prepareList(kind: EditorKind, text: string, start: number, end: number, composition: boolean) {
  if (kind === "composer") handBack(text);
  else {
    seedTask(text);
    setTaskNumber(n => n + 1);
  }
  await tick();
  const el = listEditor(kind);
  if (el.value !== text || el.readOnly || el.disabled) throw Error(kind + ": draft is not ready");
  el.focus();
  el.setSelectionRange(start, end, start === end ? "none" : "backward");
  if (composition) {
    el.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    const enter = key(el, "Enter");
    const newline = el.dispatchEvent(new InputEvent("beforeinput", { inputType: "insertLineBreak", bubbles: true, cancelable: true }));
    if (!enter || !newline || el.value !== text || submitted(kind).length) throw Error(kind + ": composition caused an edit or submission");
    el.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    await tick();
  }
  listSnapshot(kind, "list-ready");
}
async function run(){
 await loadMarkdown();await open();await tick();let el=editor();el.focus();command("insertText","hello");await tick(300);
 eq("typing reaches optimistic draft",model.text(),"hello");
 key(el,"z",{ctrlKey:true});eq("typing undo reaches draft",model.text(),"");
 key(el,"y",{ctrlKey:true});eq("typing redo reaches draft",model.text(),"hello");
 key(el,"z",{ctrlKey:true});
 key(el,"Tab");await tick();eq("real Composer Tab suggestion in empty field",el.value,"hello completion");
 key(el,"z",{ctrlKey:true});eq("Ctrl+Z undoes one suggestion",el.value,"");eq("suggestion undo reaches draft",model.text(),"");
 key(el,"z",{ctrlKey:true,shiftKey:true});eq("Ctrl+Shift+Z redoes suggestion",el.value,"hello completion");
 key(el,"z",{ctrlKey:true});key(el,"y",{ctrlKey:true});eq("Ctrl+Y redoes suggestion",el.value,"hello completion");
 // Same-value model echoes, selections and a later flush preserve the undo item.
 await tick(400);key(el,"z",{ctrlKey:true});eq("delayed replies preserve undo",el.value,"");
 command("insertText","hello");
 replaceText(el,"hello\nvoice text");eq("native multiline/voice-style insertion",model.text(),"hello\nvoice text");
 key(el,"z",{ctrlKey:true});eq("voice-style insertion undo",model.text(),"hello");
 handBack("another saved draft");await tick();el=editor();el.focus();key(el,"z",{ctrlKey:true});eq("new draft never resurrects old text",el.value,"another saved draft");
 const d=()=>document.querySelector<HTMLInputElement>("#dialog")!;d().focus();command("insertText","dialog draft");key(d(),"z",{ctrlKey:true});eq("dialog typing undo",dialog(),"");key(d(),"y",{ctrlKey:true});eq("dialog typing redo",dialog(),"dialog draft");
 const before=inputs;setDialog("remote edit");await tick();eq("focused model replacement is silent",inputs,before);key(d(),"z",{ctrlKey:true});eq("model replacement undo is a real edit",dialog(),"dialog draft");
 setEpoch(2);setDialog("other context");await tick();d().focus();key(d(),"z",{ctrlKey:true});eq("dialog context resets history",d().value,"other context");
 const c=document.querySelector<HTMLInputElement>("#change")!;c.focus();command("insertText","raw typing");await tick();eq("onChange field keeps raw typing",c.value,"raw typing");
 const i=document.querySelector<HTMLTextAreaElement>("#ime")!;i.focus();i.dispatchEvent(new CompositionEvent("compositionstart",{bubbles:true}));setIme("late remote");await tick();eq("IME defers model replacement",i.value,"");
 command("insertText","日本語");i.dispatchEvent(new CompositionEvent("compositionend",{bubbles:true}));await tick(10);eq("final IME input wins stale model",i.value,"日本語");
 const n=()=>document.querySelector<HTMLInputElement>("#number")!;n().focus();n().select();command("delete");await tick();
 eq("numeric field can become empty",n().value,"");key(n(),"z",{ctrlKey:true});eq("empty numeric field undo",n().value,"10");
 key(n(),"y",{ctrlKey:true});eq("empty numeric field redo",n().value,"");
 command("insertText","0012");eq("numeric model echoes preserve raw leading zeros",n().value,"0012");
 key(n(),"z",{ctrlKey:true});eq("raw numeric undo",n().value,"");key(n(),"y",{ctrlKey:true});eq("raw numeric redo",n().value,"0012");
 // A refused native replacement must restore the caret captured BEFORE
 // select-all, even when rebuilding an edited field is necessary.
 d().focus();d().select();command("insertText","abcdef");await tick();d().setSelectionRange(2,2);
 const native=document.execCommand.bind(document);
 try {
   document.execCommand=((name:string,show?:boolean,value?:string)=>name==="insertText"?false:native(name,show,value)) as typeof document.execCommand;
   setDialog("ABCDEF");await tick();
 } finally {document.execCommand=native;}
 eq("refused native replacement preserves original caret",[d().value,d().selectionStart,d().selectionEnd],["ABCDEF",2,2]);
 // No synthetic key here: the Python driver also runs real X11 key events.
 (window as any).textEditFixture={results,editor,model,replaceText,sends,listSnapshot,
   prepareList: (...args: Parameters<typeof prepareList>) => prepareList(...args).catch(e => post({error:String(e),stack:e.stack}))};
 post({stage:"ready",results});
}
run().catch(e=>post({error:String(e),stack:e.stack}));
