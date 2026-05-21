import React from 'react';

type Mode = 'server' | 'peer' | 'gaming' | 'share';

interface ModeSelectorProps {
  activeMode: Mode;
  onModeChange: (mode: Mode) => void;
  disabled?: boolean;
}

const modes: { id: Mode; label: string; disabled: boolean }[] = [
  { id: 'server', label: 'Server', disabled: false },
  { id: 'peer', label: 'Peer Relay', disabled: false },
  { id: 'share', label: 'Share', disabled: false },
  { id: 'gaming', label: 'Gaming', disabled: false },
];

const ModeSelector: React.FC<ModeSelectorProps> = ({ activeMode, onModeChange, disabled }) => {
  return (
    <div className="mode-selector" id="mode-selector">
      {modes.map((mode) => {
        const isTabDisabled = disabled || mode.disabled;
        return (
          <button
            key={mode.id}
            id={`mode-tab-${mode.id}`}
            className={`mode-selector__tab ${
              activeMode === mode.id ? 'mode-selector__tab--active' : ''
            } ${isTabDisabled ? 'mode-selector__tab--disabled' : ''}`}
            onClick={() => !isTabDisabled && onModeChange(mode.id)}
            disabled={isTabDisabled}
          >
            {mode.label}
          </button>
        );
      })}
    </div>
  );
};

export default ModeSelector;
export type { Mode };
