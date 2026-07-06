import { useEffect, useMemo, useRef, useState } from "react";
import { filterCommands, type PaletteCommand } from "../lib/commands";

interface Props {
  commands: PaletteCommand[];
  onRun: (id: string) => void;
  onClose: () => void;
}

// Ctrl+K overlay: type to filter, arrows to move, Enter to run, Esc to close.
export default function CommandPalette({ commands, onRun, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const matches = useMemo(
    () => filterCommands(query, commands),
    [query, commands],
  );
  const active = Math.min(cursor, Math.max(0, matches.length - 1));

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, matches.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter" && matches[active]) {
      e.preventDefault();
      onRun(matches[active].id);
    }
  };

  return (
    <div className="palette-veil" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-label="command palette"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          className="palette-input"
          value={query}
          placeholder="Type a command…"
          aria-label="search commands"
          onChange={(e) => {
            setQuery(e.target.value);
            setCursor(0);
          }}
          onKeyDown={handleKey}
        />
        <ul className="palette-list" role="listbox">
          {matches.map((cmd, i) => (
            <li key={cmd.id} role="option" aria-selected={i === active}>
              <button
                type="button"
                className="palette-item"
                data-active={i === active}
                onMouseEnter={() => setCursor(i)}
                onClick={() => onRun(cmd.id)}
              >
                <span>{cmd.label}</span>
                {cmd.hint && <span className="palette-hint">{cmd.hint}</span>}
              </button>
            </li>
          ))}
          {matches.length === 0 && (
            <li className="palette-empty">No matching command</li>
          )}
        </ul>
      </div>
    </div>
  );
}
