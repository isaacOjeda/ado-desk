// "Ask the assistant" action for detail headers: opens the panel with a prompt about the item,
// typed into the terminal but not sent, so the user finishes the question.
import { MOD_KEY } from "../../lib/platform";
import { askAssistant } from "./events";

export function AskAssistant({ prompt }: { prompt: string }) {
  return (
    <button
      className="btn btn-ghost btn-sm"
      onClick={() => askAssistant(prompt)}
      title={`Ask the assistant about this (${MOD_KEY}J opens the panel)`}
    >
      ✦ Ask
    </button>
  );
}
