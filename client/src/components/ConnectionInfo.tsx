import React from 'react';

interface ConnectionInfoProps {
  serverIp: string;
  protocol: string;
  encryption: string;
  isConnected: boolean;
}

const ConnectionInfo: React.FC<ConnectionInfoProps> = ({
  serverIp,
  protocol,
  encryption,
  isConnected,
}) => {
  if (!isConnected) return null;

  return (
    <div id="connection-info" className="connection-info glass-panel fade-in fade-in-delay-2">
      <div className="connection-info__row">
        <span className="connection-info__label">Server IP</span>
        <span className="connection-info__value connection-info__value--accent">
          {serverIp}
        </span>
      </div>
      <hr className="connection-info__divider" />
      <div className="connection-info__row">
        <span className="connection-info__label">Protocol</span>
        <span className="connection-info__value">{protocol}</span>
      </div>
      <hr className="connection-info__divider" />
      <div className="connection-info__row">
        <span className="connection-info__label">Encryption</span>
        <span className="connection-info__value">{encryption}</span>
      </div>
    </div>
  );
};

export default ConnectionInfo;
