import React from 'react';
import { ArrowUpIcon, ArrowDownIcon } from './Icons';

interface TrafficStatsProps {
  bytesIn: number;
  bytesOut: number;
}

function formatBytes(bytes: number): { value: string; unit: string } {
  if (bytes < 1024) return { value: bytes.toFixed(0), unit: 'B' };
  if (bytes < 1024 * 1024) return { value: (bytes / 1024).toFixed(1), unit: 'KB' };
  if (bytes < 1024 * 1024 * 1024) return { value: (bytes / (1024 * 1024)).toFixed(1), unit: 'MB' };
  return { value: (bytes / (1024 * 1024 * 1024)).toFixed(2), unit: 'GB' };
}

const TrafficStats: React.FC<TrafficStatsProps> = ({ bytesIn, bytesOut }) => {
  const download = formatBytes(bytesIn);
  const upload = formatBytes(bytesOut);

  return (
    <div className="traffic-stats" id="traffic-stats">
      <div className="traffic-stat">
        <ArrowDownIcon className="traffic-stat__arrow traffic-stat__arrow--down" />
        <span className="traffic-stat__value">{download.value}</span>
        <span className="traffic-stat__unit">{download.unit}</span>
      </div>
      <div className="traffic-stat">
        <ArrowUpIcon className="traffic-stat__arrow traffic-stat__arrow--up" />
        <span className="traffic-stat__value">{upload.value}</span>
        <span className="traffic-stat__unit">{upload.unit}</span>
      </div>
    </div>
  );
};

export default TrafficStats;
