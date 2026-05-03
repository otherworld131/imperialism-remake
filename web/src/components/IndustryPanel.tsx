import { useState, useCallback, useRef } from 'react';
import type { IndustryData, BuildableUnits } from '../wasm';

interface Props {
  industry: IndustryData;
  buildable: BuildableUnits | null;
  onExpand: (buildingType: string) => void;
  onRecruit: (unitType: string) => void;
  onBuildShip: (shipType: string) => void;
  onHire: (civilianType: string) => void;
  onBuildFreightCar: () => void;
}

const RESOURCE_EMOJI: Record<string, string> = {
  Timber: '🪵', Lumber: '🪵', Coal: '🪨', Iron: '🔩', Steel: '⚙️',
  Cotton: '🌸', Wool: '🐑', Fabric: '🧵', Clothing: '👗',
  Furniture: '🛋️', Hardware: '🔧', Oil: '🛢️', Horses: '🐴',
  Food: '🌾', Grain: '🌾', Fish: '🐟', Gold: '🪙', Gems: '💎',
  Saltpeter: '💨', Rubber: '🌿', Copper: '🟤', Arms: '🗡️',
  FreightCars: '🚃',
};

const CHAIN_CONFIG = [
  {
    key: 'timber_chain' as const,
    name: 'Timber', emoji: '🪵',
    mill: { label: 'Timber → Lumber' },
    factory: { label: 'Lumber → Furniture' },
  },
  {
    key: 'metal_chain' as const,
    name: 'Metal', emoji: '⚙️',
    mill: { label: 'Coal+Iron → Steel' },
    factory: { label: 'Steel → Hardware' },
  },
  {
    key: 'textile_chain' as const,
    name: 'Textile', emoji: '🧵',
    mill: { label: 'Cotton/Wool → Fabric' },
    factory: { label: 'Fabric → Clothing' },
  },
];

const UNIT_EMOJI: Record<string, string> = {
  Infantry: '⚔️', Cavalry: '🐎', Artillery: '💣', Special: '⭐', Garrison: '🛡️',
};

const SHIP_EMOJI: Record<string, string> = {
  Merchant: '🚢', Warship: '⚓',
};

const CIV_EMOJI: Record<string, string> = {
  Farmer: '🌾', Miner: '⛏️', Engineer: '🔧', Forester: '🪓', Rancher: '🤠',
  Driller: '🛢️', Prospector: '🔍',
};

export default function IndustryPanel({
  industry, buildable,
  onExpand, onRecruit, onBuildShip, onHire, onBuildFreightCar,
}: Props) {
  const { buildings, warehouse, labor, production_forecast, can_expand } = industry;
  const treasury = buildable?.treasury ?? 0;
  const arms = buildable?.arms ?? 0;
  const totalLabor = labor.total_labor_units;

  return (
    <div style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
      {/* Treasury & Arms header */}
      <div style={{ display: 'flex', gap: 12, marginBottom: 12, color: '#daa520', fontWeight: 'bold' }}>
        <span>💰 ${treasury.toLocaleString()}</span>
        <span>⚔️ Arms: {arms}</span>
      </div>

      {/* 3-column grid */}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 16, alignItems: 'start' }}>

        {/* Column 1: Production Chains + Labor */}
        <div>
          <Section label="Production Chains" emoji="🏭">
            {CHAIN_CONFIG.map(chain => {
              const forecast = production_forecast[chain.key];
              return (
                <div key={chain.key} style={{ marginBottom: 8 }}>
                  <div style={{ fontWeight: 'bold', color: '#daa520', marginBottom: 3 }}>
                    {chain.emoji} {chain.name}
                  </div>
                  <ChainStepRow
                    label={chain.mill.label}
                    labor={forecast.mill_labor}
                    totalLabor={totalLabor}
                    output={forecast.mill_output}
                  />
                  <ChainStepRow
                    label={chain.factory.label}
                    labor={forecast.factory_labor}
                    totalLabor={totalLabor}
                    output={forecast.factory_output}
                  />
                </div>
              );
            })}
          </Section>

          <Section label="Labor" emoji="👷">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <LaborRow emoji="👕" label="Untrained" count={labor.untrained} color="#888" />
              <LaborRow emoji="👔" label="Trained" count={labor.trained} color="#6ab0d4" />
              <LaborRow emoji="🥼" label="Expert" count={labor.expert} color="#4a8fd4" />
              <div style={{ borderTop: '1px solid #333', paddingTop: 4, marginTop: 2, color: '#daa520', fontSize: 11 }}>
                = {totalLabor} labor units
              </div>
            </div>
          </Section>
        </div>

        {/* Column 2: Buildings + Warehouse */}
        <div>
          <Section label="Buildings" emoji="🏗️">
            {buildings.map(b => {
              const expandable = can_expand[b.type] ?? false;
              return (
                <div key={b.type} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '2px 0', marginBottom: 2 }}>
                  <div>
                    <span>{b.display_name}</span>
                    <span style={{ fontSize: 10, color: '#888', marginLeft: 4 }}>
                      {b.capacity}{b.is_expanding ? `→${b.capacity + b.pending_capacity}` : `/${b.next_capacity}`}
                    </span>
                  </div>
                  {b.is_expanding ? (
                    <span style={{ fontSize: 10, color: '#daa520' }}>{b.turns_remaining}t left</span>
                  ) : (
                    <button onClick={() => onExpand(b.type)} disabled={!expandable}
                      style={btnStyle(expandable ? '#456' : '#333')}
                      title={`Cost: ${b.expansion_cost.lumber}L + ${b.expansion_cost.steel}S`}>
                      Expand
                    </button>
                  )}
                </div>
              );
            })}
          </Section>

          <Section label="Warehouse" emoji="📦">
            <WarehouseSection label="Resources" items={warehouse.resources} />
            <WarehouseSection label="Materials" items={warehouse.materials} />
            <WarehouseSection label="Goods" items={warehouse.goods} />
          </Section>
        </div>

        {/* Column 3: Logistics + Army + Naval + Civilians */}
        <div>
          <Section label="Logistics" emoji="🚂">
            <SliderRow
              emoji="🚃"
              label="Freight Car"
              sublabel="1L + 1S + 2 labor"
              canAfford={(warehouse.materials['Lumber'] ?? 0) >= 1 && (warehouse.materials['Steel'] ?? 0) >= 1 && labor.total_labor_units >= 2}
              reason={(warehouse.materials['Lumber'] ?? 0) < 1 ? 'Need 1 lumber' : (warehouse.materials['Steel'] ?? 0) < 1 ? 'Need 1 steel' : labor.total_labor_units < 2 ? 'Need 2 labor units' : null}
              onCommit={onBuildFreightCar}
            />
          </Section>

          {buildable && buildable.army.filter(b => b.tech_met).length > 0 && (
            <Section label="Army Recruitment" emoji="⚔️">
              {buildable.army.filter(b => b.tech_met).map(b => (
                <SliderRow
                  key={b.type}
                  emoji={UNIT_EMOJI[b.category ?? ''] ?? '⚔️'}
                  label={fmtName(b.type)}
                  sublabel={`$${b.cost}${b.arms_required ? ` + ${b.arms_required}A` : ''}`}
                  canAfford={b.can_afford}
                  reason={!b.can_afford ? (b.reason ?? 'Cannot afford') : null}
                  onCommit={() => onRecruit(b.type)}
                />
              ))}
            </Section>
          )}

          {buildable && buildable.ships.filter(b => b.tech_met).length > 0 && (
            <Section label="Naval Construction" emoji="⚓">
              {['Warship', 'Merchant'].map(cat => {
                const ships = buildable.ships.filter(b => b.category === cat && b.tech_met);
                if (ships.length === 0) return null;
                return (
                  <div key={cat}>
                    <div style={{ fontSize: 10, color: '#888', textTransform: 'uppercase', marginBottom: 2 }}>{cat}s</div>
                    {ships.map(b => (
                      <SliderRow
                        key={b.type}
                        emoji={SHIP_EMOJI[cat] ?? '⚓'}
                        label={fmtName(b.type)}
                        sublabel={b.resources_needed
                          ? Object.entries(b.resources_needed).map(([k, v]) => `${v}${k[0].toUpperCase()}`).join('+')
                          : ''}
                        canAfford={b.can_afford}
                        reason={!b.can_afford ? (b.reason ?? 'Cannot afford') : null}
                        onCommit={() => onBuildShip(b.type)}
                      />
                    ))}
                  </div>
                );
              })}
            </Section>
          )}

          {buildable && buildable.civilians.filter(b => b.tech_met).length > 0 && (
            <Section label="Civilian Hiring" emoji="🧑‍🌾">
              {buildable.civilians.filter(b => b.tech_met).map(b => (
                <SliderRow
                  key={b.type}
                  emoji={CIV_EMOJI[b.type] ?? '👷'}
                  label={b.type}
                  sublabel={`$${b.cost}`}
                  canAfford={b.can_afford}
                  reason={!b.can_afford ? (b.reason ?? 'Cannot afford') : null}
                  onCommit={() => onHire(b.type)}
                />
              ))}
            </Section>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────

function Section({ label, emoji, children }: { label: string; emoji: string; children: React.ReactNode }) {
  return (
    <div style={{ borderTop: '1px solid #3a3520', paddingTop: 6, marginTop: 6 }}>
      <div style={{ fontWeight: 'bold', marginBottom: 4, color: '#ccc' }}>{emoji} {label}</div>
      {children}
    </div>
  );
}

function ChainStepRow({ label, labor, totalLabor, output }: {
  label: string; labor: number; totalLabor: number; output: number;
}) {
  const pct = totalLabor > 0 ? Math.min((labor / totalLabor) * 100, 100) : 0;
  return (
    <div style={{ marginBottom: 5 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, color: '#bbb' }}>
        <span>{label}</span>
        <span style={{ color: '#e0d8c0', flexShrink: 0, marginLeft: 4 }}>{output}</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 2 }}>
        <div style={{ flex: 1, height: 4, background: '#2a2a2a', borderRadius: 2 }}>
          <div style={{ width: `${pct}%`, height: '100%', background: '#daa520', borderRadius: 2 }} />
        </div>
        <span style={{ fontSize: 9, color: '#666', whiteSpace: 'nowrap' }}>👷 {labor}</span>
      </div>
    </div>
  );
}

function LaborRow({ emoji, label, count, color }: { emoji: string; label: string; count: number; color: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
      <span style={{ fontSize: 16, lineHeight: 1 }}>{emoji}</span>
      <span style={{ color: '#aaa', flex: 1 }}>{label}</span>
      <span style={{ color, fontWeight: 'bold' }}>{count}</span>
    </div>
  );
}

interface SliderRowProps {
  emoji: string;
  label: string;
  sublabel: string;
  canAfford: boolean;
  reason: string | null;
  onCommit: () => void;
}

function SliderRow({ emoji, label, sublabel, canAfford, reason, onCommit }: SliderRowProps) {
  const [value, setValue] = useState(0);
  const committed = useRef(false);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const v = Number(e.target.value);
    setValue(v);
    committed.current = false;
  }, []);

  const handleCommit = useCallback(() => {
    if (value > 0 && canAfford && !committed.current) {
      committed.current = true;
      onCommit();
      setValue(0);
    }
  }, [value, canAfford, onCommit]);

  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 6,
      padding: '3px 0', opacity: canAfford ? 1 : 0.45,
    }}>
      <span style={{ width: 18, textAlign: 'center' }}>{emoji}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ fontSize: 'var(--ui-font-size, 14px)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {label}
          </span>
          <span style={{ fontSize: 10, color: '#888', marginLeft: 4, flexShrink: 0 }}>{sublabel}</span>
        </div>
        {canAfford ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 2 }}>
            <input
              type="range"
              min={0}
              max={1}
              step={1}
              value={value}
              onChange={handleChange}
              onMouseUp={handleCommit}
              onTouchEnd={handleCommit}
              style={{ flex: 1, cursor: 'pointer', accentColor: '#daa520', height: 4 }}
            />
            <span style={{ fontSize: 10, color: value > 0 ? '#daa520' : '#555', width: 12, textAlign: 'center' }}>
              {value > 0 ? '▶' : ''}
            </span>
          </div>
        ) : (
          <div style={{ fontSize: 10, color: '#a66', marginTop: 1 }}>{reason}</div>
        )}
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
          <span key={k} style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
            {RESOURCE_EMOJI[k] ?? '📦'} {k.replace(/([A-Z])/g, ' $1').trim()}: <span style={{ color: '#daa520' }}>{v}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

function fmtName(s: string) {
  return s.replace(/([A-Z])/g, ' $1').trim();
}

function btnStyle(bg: string): React.CSSProperties {
  return { background: bg, color: '#fff', border: 'none', borderRadius: 3, padding: '1px 6px', fontSize: 10, cursor: 'pointer' };
}
