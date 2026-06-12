import { Play } from "lucide-react";

type ChoiceListProps = {
  choices: string[];
  onChoice: (choice: string) => void;
};

export function ChoiceList({ choices, onChoice }: ChoiceListProps) {
  if (choices.length === 0) return null;

  return (
    <div className="choice-list" role="list">
      {choices.slice(0, 2).map((choice) => (
        <button
          className="choice-button"
          key={choice}
          onClick={() => onChoice(choice)}
          type="button"
        >
          <Play className="choice-icon" size={16} fill="currentColor" />
          <span>{choice}</span>
        </button>
      ))}
    </div>
  );
}
