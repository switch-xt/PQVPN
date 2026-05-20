import React, { useState } from 'react';
import { ActivityIcon, ChevronDownIcon, ClockIcon, LockIcon, WifiIcon, ZapIcon } from './Icons';

interface IntegrityData {
  lastHandshake: string;
  pskRotationCountdown: number; // seconds
  bytesIn: number;
  bytesOut: number;
  packetLoss: number; // percentage
  tunnelUptime: string;
  pskAgeSeconds: number;
}

interface IntegrityMonitorProps {
  data: IntegrityData;
  isConnected: boolean;
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

const IntegrityMonitor: React.FC<IntegrityMonitorProps> = ({ data, isConnected }) => {
  const [expanded, setExpanded] = useState(false);

  if (!isConnected) return null;

  return (
    <div
      id="integrity-monitor"
      className={`integrity-monitor glass-panel fade-in fade-in-delay-3 ${
        expanded ? 'integrity-monitor--expanded' : ''
      }`}
    >
      <div
        className="integrity-monitor__header"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="integrity-monitor__title">
          <ActivityIcon className="integrity-monitor__title-icon" />
          <span>Integrity Monitor</span>
        </div>
        <ChevronDownIcon className="integrity-monitor__chevron" />
      </div>

      {expanded && (
        <div className="integrity-monitor__body">
          <div className="integrity-monitor__stat">
            <span className="integrity-monitor__stat-label">
              <ClockIcon style={{ width: 12, height: 12 }} />
              Last Handshake
            </span>
            <span className="integrity-monitor__stat-value">
              {data.lastHandshake}
            </span>
          </div>

          <div className="integrity-monitor__stat">
            <span className="integrity-monitor__stat-label">
              <LockIcon style={{ width: 12, height: 12 }} />
              PSK Age
            </span>
            <span className="integrity-monitor__stat-value">
              {formatDuration(data.pskAgeSeconds)}
            </span>
          </div>

          <div className="integrity-monitor__stat">
            <span className="integrity-monitor__stat-label">
              <ZapIcon style={{ width: 12, height: 12 }} />
              PSK Rotation In
            </span>
            <span className="integrity-monitor__stat-value">
              {formatDuration(data.pskRotationCountdown)}
            </span>
          </div>

          <div className="integrity-monitor__stat">
            <span className="integrity-monitor__stat-label">
              <WifiIcon style={{ width: 12, height: 12 }} />
              Packet Loss
            </span>
            <span className="integrity-monitor__stat-value">
              {data.packetLoss.toFixed(1)}%
            </span>
          </div>

          <div className="integrity-monitor__stat">
            <span className="integrity-monitor__stat-label">
              <ClockIcon style={{ width: 12, height: 12 }} />
              Tunnel Uptime
            </span>
            <span className="integrity-monitor__stat-value">
              {data.tunnelUptime}
            </span>
          </div>
        </div>
      )}
    </div>
  );
};

export default IntegrityMonitor;
export type { IntegrityData };
