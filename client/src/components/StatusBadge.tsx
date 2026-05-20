import React from 'react';

type PqcStatus = 'active' | 'off' | 'degraded';

interface StatusBadgeProps {
  status: PqcStatus;
}

const statusLabels: Record<PqcStatus, string> = {
  active: 'PQC Active',
  off: 'PQC Off',
  degraded: 'PQC Degraded',
};

const StatusBadge: React.FC<StatusBadgeProps> = ({ status }) => {
  return (
    <div
      id="pqc-status-badge"
      className={`status-badge status-badge--pqc-${status}`}
    >
      <span className="status-badge__dot" />
      <span>{statusLabels[status]}</span>
    </div>
  );
};

export default StatusBadge;
export type { PqcStatus };
