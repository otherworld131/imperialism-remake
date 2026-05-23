import type { TransportData } from '../wasm';
import { resourceLabel, resourceEmoji } from '../resourceEmoji';

interface Props {
  transport: TransportData;
  onSetAllocation: (resource: string, units: number) => void;
}

export default function TransportPanel({ transport, onSetAllocation }: Props) {
  const { total_capacity, remote_delivery_capacity, military_transport_capacity, allocations, deliveries, demand = [], local_deliveries = [], food_requirement } = transport;
  const transportDeliveries = deliveries;

  const allocationMap: Record<string, number> = {};
  for (const a of allocations) allocationMap[a.resource] = a.units;

  const demandMap: Record<string, number> = {};
  for (const d of demand) demandMap[d.resource] = d.demand;

  const cap = remote_delivery_capacity ?? total_capacity;
  const allocatedUnitsByResource: Record<string, number> = {};
  for (const d of transportDeliveries) {
    allocatedUnitsByResource[d.resource] = allocationMap[d.resource] ?? 0;
  }
  const totalAllocatedUnits = Object.values(allocationMap).reduce((sum, units) => sum + units, 0);
  const remainingCapacity = Math.max(0, cap - totalAllocatedUnits);

  return (
    <div style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6 }}>Freight Cars</div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
        <span>Capacity: {remainingCapacity} ({cap})</span>
      </div>

      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Transport Allocation</div>
        {transportDeliveries.length === 0 && (
          <div style={{ color: '#888', fontStyle: 'italic', fontSize: 'var(--ui-font-size, 14px)' }}>No resources available</div>
        )}
        {transportDeliveries.map(d => {
          const allocatedUnits = allocatedUnitsByResource[d.resource] ?? 0;
          const projectedDelivery = Math.min(allocatedUnits, d.available);
          const demandQty = demandMap[d.resource] ?? 0;
          const belowDemand = demandQty > 0 && projectedDelivery < demandQty;
          const canDecrease = allocatedUnits > 0;
          const canIncrease = remainingCapacity > 0 && cap > 0;
          return (
            <div key={d.resource} style={{
              display: 'flex', alignItems: 'center', gap: 4, marginBottom: 3,
              background: belowDemand ? 'rgba(220,50,50,0.10)' : 'rgba(255,255,255,0.03)',
              borderRadius: 3, padding: '3px 4px',
              border: belowDemand ? '1px solid rgba(220,50,50,0.4)' : '1px solid transparent',
            }}>
              <span style={{ flex: 1, fontSize: 'var(--ui-font-size, 14px)' }}>{resourceLabel(d.resource)}</span>
              <button
                onClick={() => onSetAllocation(d.resource, allocatedUnits - 1)}
                disabled={!canDecrease}
                style={smallBtn(!canDecrease)}
              >-</button>
              <span style={{ width: 40, textAlign: 'center', fontSize: 11 }}>{allocatedUnits}/{d.available}</span>
              <button
                onClick={() => onSetAllocation(d.resource, allocatedUnits + 1)}
                disabled={!canIncrease}
                style={smallBtn(!canIncrease)}
              >+</button>
              {belowDemand && (
                <span style={{ fontSize: 9, color: '#e44', marginLeft: 'auto' }} title={`Demand: ${demandQty}`}>
                  ▼{demandQty}
                </span>
              )}
              {!belowDemand && (
                <span style={{ width: 24, marginLeft: 'auto' }} />
              )}
            </div>
          );
        })}
      </div>

      {local_deliveries.length > 0 && (
        <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
          <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Capital Tile Delivery</div>
          {local_deliveries.map(d => (
            <div key={d.resource} style={{
              display: 'flex', alignItems: 'center', gap: 4, marginBottom: 3,
              background: 'rgba(255,255,255,0.03)', borderRadius: 3, padding: '3px 4px',
            }}>
              <span style={{ flex: 1, fontSize: 'var(--ui-font-size, 14px)' }}>{resourceLabel(d.resource)}</span>
              <span style={{ fontSize: 11, color: '#999' }}>{d.delivered}/{d.available}</span>
            </div>
          ))}
        </div>
      )}

      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Military Transport</div>
        <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#aaa' }}>
          Rail capacity: {military_transport_capacity} unit{military_transport_capacity !== 1 ? 's' : ''}
        </div>
      </div>

      {food_requirement && food_requirement.workers > 0 && (
        <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
          <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Food Requirements</div>
          <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#aaa', marginBottom: 3 }}>
            👷 {food_requirement.workers.toLocaleString()} worker{food_requirement.workers !== 1 ? 's' : ''}
          </div>
          <FoodRow label={resourceLabel('Grain')} qty={food_requirement.grain} />
          <FoodRow label={resourceLabel('Fruit')} qty={food_requirement.fruit} />
          <FoodRow
            label={`${resourceEmoji('Livestock')}${resourceEmoji('Fish')} Meat`}
            qty={food_requirement.meat}
          />
        </div>
      )}
    </div>
  );
}

function FoodRow({ label, qty }: { label: string; qty: number }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      fontSize: 'var(--ui-font-size, 14px)', color: '#aaa',
      padding: '1px 0',
    }}>
      <span>{label}</span>
      <span>{qty.toLocaleString()}</span>
    </div>
  );
}

function smallBtn(disabled: boolean): React.CSSProperties {
  return {
    background: disabled ? '#26211a' : '#3a3520',
    color: disabled ? '#7f7765' : '#e0d8c0',
    border: 'none',
    borderRadius: 2,
    padding: '1px 5px',
    fontSize: 11,
    cursor: disabled ? 'default' : 'pointer',
    minWidth: 18,
  };
}
