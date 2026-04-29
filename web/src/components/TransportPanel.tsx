import type { TransportData } from '../wasm';
import { resourceLabel } from '../resourceEmoji';

interface Props {
  transport: TransportData;
  onBuildCar: () => void;
  onSetAllocation: (resource: string, percentage: number) => void;
}

export default function TransportPanel({ transport, onBuildCar, onSetAllocation }: Props) {
  const { freight_cars, total_capacity, military_transport_capacity, allocations, build_cost, can_build, deliveries } = transport;

  const allocationMap: Record<string, number> = {};
  for (const a of allocations) allocationMap[a.resource] = a.percentage;

  const totalPct = Object.values(allocationMap).reduce((s, v) => s + v, 0);

  return (
    <div style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6 }}>Freight Cars</div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
        <span>Cars: {freight_cars}</span>
        <span>Capacity: {total_capacity}</span>
      </div>
      <div style={{ marginBottom: 6 }}>
        <button
          onClick={onBuildCar}
          disabled={!can_build}
          style={btnStyle(can_build ? '#2a6' : '#444')}
        >
          Build Car
        </button>
        <span style={{ fontSize: 10, color: '#888', marginLeft: 6 }}>
          {build_cost.lumber}L + {build_cost.steel}S
        </span>
      </div>

      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Resource Allocation</div>
        {totalPct > 100 && (
          <div style={{ color: '#e44', fontSize: 11, marginBottom: 4 }}>
            Total exceeds 100% ({totalPct}%)
          </div>
        )}
        {deliveries.length === 0 && (
          <div style={{ color: '#888', fontStyle: 'italic', fontSize: 'var(--ui-font-size, 14px)' }}>No resources available</div>
        )}
        {deliveries.map(d => {
          const pct = allocationMap[d.resource] ?? 0;
          return (
            <div key={d.resource} style={{
              display: 'flex', alignItems: 'center', gap: 4, marginBottom: 3,
              background: 'rgba(255,255,255,0.03)', borderRadius: 3, padding: '3px 4px',
            }}>
              <span style={{ flex: 1, fontSize: 'var(--ui-font-size, 14px)' }}>{resourceLabel(d.resource)}</span>
              <button
                onClick={() => onSetAllocation(d.resource, Math.max(0, pct - 10))}
                style={smallBtn}
              >-</button>
              <span style={{ width: 32, textAlign: 'center', fontSize: 11 }}>{pct}%</span>
              <button
                onClick={() => onSetAllocation(d.resource, Math.min(100, pct + 10))}
                style={smallBtn}
              >+</button>
              <span style={{ fontSize: 10, color: '#999', width: 36, textAlign: 'right' }}>
                {d.delivered}/{d.available}
              </span>
            </div>
          );
        })}
      </div>

      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Military Transport</div>
        <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#aaa' }}>
          Rail capacity: {military_transport_capacity} unit{military_transport_capacity !== 1 ? 's' : ''}
        </div>
      </div>
    </div>
  );
}

function btnStyle(bg: string): React.CSSProperties {
  return {
    background: bg, color: '#fff', border: 'none', borderRadius: 3,
    padding: '2px 8px', fontSize: 11, cursor: 'pointer',
  };
}

const smallBtn: React.CSSProperties = {
  background: '#3a3520', color: '#e0d8c0', border: 'none', borderRadius: 2,
  padding: '1px 5px', fontSize: 11, cursor: 'pointer', minWidth: 18,
};
