import React from 'react';
import { GlobeIcon, ChevronRightIcon } from './Icons';

interface ServerSelectorProps {
  serverName: string;
  serverLocation: string;
  flag: string;
  onClick?: () => void;
}

const ServerSelector: React.FC<ServerSelectorProps> = ({
  serverName,
  serverLocation,
  flag,
  onClick,
}) => {
  return (
    <div
      id="server-selector"
      className="server-selector glass-panel fade-in fade-in-delay-2"
      onClick={onClick}
    >
      <div className="server-selector__info">
        <div className="server-selector__flag">
          {flag || <GlobeIcon style={{ width: 16, height: 16 }} />}
        </div>
        <div className="server-selector__details">
          <span className="server-selector__name">{serverName}</span>
          <span className="server-selector__location">{serverLocation}</span>
        </div>
      </div>
      <ChevronRightIcon className="server-selector__arrow" />
    </div>
  );
};

export default ServerSelector;
