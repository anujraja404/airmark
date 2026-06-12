import type { Message } from "../types";
import { ChoiceList } from "./ChoiceList";

type DialogBoxProps = {
  message: Message;
  onChoice: (choice: string) => void;
};

export function DialogBox({ message, onChoice }: DialogBoxProps) {
  return (
    <article className={`dialog-box ${message.role}`}>
      <p>{message.content}</p>
      {message.role === "assistant" && (
        <ChoiceList choices={message.choices ?? []} onChoice={onChoice} />
      )}
    </article>
  );
}
