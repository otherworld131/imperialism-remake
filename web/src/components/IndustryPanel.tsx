import type { IndustryData } from '../wasm';

interface Props {
  industry: IndustryData;
  onExpand: (buildingType: string) => void;
}

const CHAIN_LABELS: Record<string, { name: string; mill: string; factory: string }> = {
  timber_chain: { name: 'Timber', mill: 'Timber \u2192 Lumber', factory: 'Lumber \u2192 Furniture' },
  metal_chain: { name: 'Metal', mill: 'Coal+Iron \u2192 Steel', factory: 'Steel \u2192 Hardware' },
  textile_chain: { name: 'Textile', mill: 'Cotton/Wool \u2192 Fabric', factory: 'Fabric \u2192 Clothing' },
};

export default function IndustryPanel({ industry, onExpand }: Props) {
  const { buildings, warehouse, labor, production_forecast, can_expand } = industry;

  return (
    <div style={{ fontSize: 13 }}>
      {/* Production Chains */}
      <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Production Chains</div>
      {Object.entries(CHAIN_LABELS).map(([key, label]) => {
        const forecast = production_forecast[key as keyof typeof production_forecast];
        return (
          <div key={key} style={{
            background: 'rgba(255,255,255,0.03)', borderRadius: 3,
            padding: '3px 5px', marginBottom: 3,
          }}>
            <div style={{ fontWeight: 'bold', fontSize: 11, color: '#daa520' }}>{label.name}</div>
            <div style={{ fontSize: 11, color: '#bbb' }}>
              {label.mill}: <span style={{ color: '#e0d8c0' }}>{forecast.mill_output}</span>
            </div>
            <div style={{ fontSize: 11, color: '#bbb' }}>
              {label.factory}: <span style={{ color: '#e0d8c0' }}>{forecast.factory_output}</span>
            </div>
          </div>
        );
      })}

      {/* Buildings */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Buildings</div>
        {buildings.map(b => {
          const expandable = can_expand[b.type] ?? false;
          return (
            <div key={b.type} style={{
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              padding: '2px 0', marginBottom: 2,
            }}>
              <div>
                <span style={{ fontSize: 12 }}>{b.display_name}</span>
                <span style={{ fontSize: 10, color: '#888', marginLeft: 4 }}>
                  {b.capacity}{b.is_expanding ? `\u2192${b.capacity + b.pending_capacity}` : `/${b.next_capacity}`}
                </span>
              </div>
              {b.is_expanding ? (
                <span style={{ fontSize: 10, color: '#daa520' }}>
                  {b.turns_remaining}t left
                </span>
              ) : (
                <button
                  onClick={() => onExpand(b.type)}
                  disabled={!expandable}
                  style={btnStyle(expandable ? '#456' : '#333')}
                  title={`Cost: ${b.expansion_cost.lumber}L + ${b.expansion_cost.steel}S`}
                >
                  Expand
                </button>
              )}
            </div>
          );
        })}
      </div>

      {/* Warehouse */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Warehouse</div>
        <WarehouseSection label="Resources" items={warehouse.resources} />
        <WarehouseSection label="Materials" items={warehouse.materials} />
        <WarehouseSection label="Goods" items={warehouse.goods} />
      </div>

      {/* Labor */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Labor</div>
        <div style={{ fontSize: 11, color: '#aaa' }}>
          {labor.untrained} untrained, {labor.trained} trained, {labor.expert} expert
          <span style={{ color: '#daa520', marginLeft: 4 }}>= {labor.total_labor_units} units</span>
        </div>
      </div>
    </div>
  );
}

function WarehouseSection({ label, items }: { label: string; items: Record<string, number> }) {
  const entries = Object.entries(items).filter(([, v]) => v > 0);
  if (entries.length === 0) return null;
  return (
    <div style={{ marginBottom: 4 }}>
      <div style={{ fontSize: 10, color: '#888', textTransform: 'uppercase', marginBottom: 2 }}>{label}</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '2px 8px' }}>
        {entries.map(([k, v]) => (
          <span key={k} style={{ fontSize: 11 }}>
            {k.replace(/([A-Z])/g, ' $1').trim()}: <span style={{ color: '#daa520' }}>{v}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

function btnStyle(bg: string): React.CSSProperties {
  return {
    background: bg, color: '#fff', border: 'none', borderRadius: 3,
    padding: '1px 6px', fontSize: 10, cursor: 'pointer',
  };
}
