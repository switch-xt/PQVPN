import React from 'react';

type ConnectionState = 'disconnected' | 'connecting' | 'connected';

interface ConnectButtonProps {
  state: ConnectionState;
  onClick: () => void;
}

const ConnectButton: React.FC<ConnectButtonProps> = ({ state, onClick }) => {
  const isConnected = state === 'connected';
  const isConnecting = state === 'connecting';

  return (
    <div className="connect-section">
      <button
        id="connect-button"
        className={`power-btn power-btn--${state}`}
        onClick={onClick}
        aria-label={isConnected ? 'Disconnect' : 'Connect'}
      >
        {/* Outer glow pulse */}
        <div className="power-btn__glow" />

        {/* Animated ring 1 — outer */}
        <div className="power-btn__ring power-btn__ring--outer" />

        {/* Animated ring 2 — middle dashed */}
        <div className="power-btn__ring power-btn__ring--mid" />

        {/* Inner glass circle */}
        <div className="power-btn__face">
          {/* Power icon — pure SVG */}
          <svg
            className="power-btn__icon"
            viewBox="0 0 24 24"
            fill="none"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M18.36 6.64a9 9 0 1 1-12.73 0" stroke="currentColor" />
            <line x1="12" y1="2" x2="12" y2="12" stroke="currentColor" />
          </svg>

          {/* Label */}
          <span className="power-btn__label">
            {isConnecting ? 'Connecting' : isConnected ? 'Secured' : 'Connect'}
          </span>
        </div>
      </button>
    </div>
  );
};

export default ConnectButton;
export type { ConnectionState };
