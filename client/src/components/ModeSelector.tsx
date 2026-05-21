import React from 'react';

type Mode = 'server' | 'peer' | 'gaming' | 'share';

interface ModeSelectorProps {
  activeMode: Mode;
  onModeChange: (mode: Mode) => void;
}

const modes: { id: Mode; label: string; disabled: boolean }[] = [
  { id: 'server', label: 'Server', disabled: false },
  { id: 'peer', label: 'Peer Relay', disabled: false },
  { id: 'share', label: 'Share', disabled: false },
  { id: 'gaming', label: 'Gaming', disabled: false },
];

const ModeSelector: React.FC<ModeSelectorProps> = ({ activeMode, onModeChange }) => {
  return (
    <div className="mode-selector" id="mode-selector">
      {modes.map((mode) => (
        <button
          key={mode.id}
          id={`mode-tab-${mode.id}`}
          className={`mode-selector__tab ${
            activeMode === mode.id ? 'mode-selector__tab--active' : ''
          } ${mode.disabled ? 'mode-selector__tab--disabled' : ''}`}
          onClick={() => !mode.disabled && onModeChange(mode.id)}
          disabled={mode.disabled}
        >
          {mode.label}
        </button>
      ))}
    </div>
  );
};

export default ModeSelector;
export type { Mode };
