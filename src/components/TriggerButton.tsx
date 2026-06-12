import { MessageSquare } from "lucide-react";
import { motion } from "framer-motion";

type TriggerButtonProps = {
  label: string;
  onClick: () => void;
};

export function TriggerButton({ label, onClick }: TriggerButtonProps) {
  return (
    <motion.button
      aria-label={label}
      className="trigger-button"
      onClick={onClick}
      whileHover={{ scale: 1.06 }}
      whileTap={{ scale: 0.94 }}
      transition={{ type: "spring", stiffness: 420, damping: 24 }}
    >
      <MessageSquare size={24} strokeWidth={2} />
    </motion.button>
  );
}
