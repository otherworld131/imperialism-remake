import { useState, useCallback } from 'react';
import type { IndustryData, BuildableUnits, ChainForecast } from '../wasm';
import { CHAIN_TARGET_UNLIMITED } from '../wasm';

interface Props {
  industry: IndustryData;
  buildable: BuildableUnits | null;
  onExpand: (buildingType: string) => void;
  onRecruit: (unitType: string) => void;
  onBuildShip: (shipType: string) => void;
  onSetPendingCivilianHire: (civilianType: string, count: number) => void;
  onBuildFreightCar: () => void;
  onSetChainTarget: (chain: string, step: string, target: number) => void;
  onSetPendingTraining: (toTrained: number, toExpert: number) => void;
}

const RESOURCE_EMOJI: Record<string, string> = {
  Timber: '🪵', Lumber: '🪵', Coal: '🪨', Iron: '🔩', Steel: '⚙️',
  Cotton: '🌸', Wool: '🐑', Fabric: '🧵', Clothing: '👗',
  Furniture: '🛋️', Hardware: '🔧', Oil: '🛢️', Horses: '🐴',
  Food: '🌾', Grain: '🌾', Fish: '🐟', Gold: '🪙', Gems: '💎',
  Saltpeter: '💨', Rubber: '🌿', Copper: '🟤', Arms: '🗡️',
  Paper: '📄', FreightCars: '🚃',
};

const CHAIN_CONFIG = [
  {
    key: 'timber_chain' as const,
    chain: 'timber',
    name: 'Timber', emoji: '🪵',
    mill: { label: 'Timber → Lumber', millKey: 'timber_mill' as const },
    factory: { label: 'Lumber → Furniture', factoryKey: 'lumber_factory' as const },
  },
  {
    key: 'metal_chain' as const,
    chain: 'metal',
    name: 'Metal', emoji: '⚙️',
    mill: { label: 'Coal+Iron → Steel', millKey: 'metal_mill' as const },
    factory: { label: 'Steel → Hardware', factoryKey: 'steel_factory' as const },
  },
  {
    key: 'textile_chain' as const,
    chain: 'textile',
    name: 'Textile', emoji: '🧵',
    mill: { label: 'Cotton/Wool → Fabric', millKey: 'textile_mill' as const },
    factory: { label: 'Fabric → Clothing', factoryKey: 'garment_factory' as const },
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
  onExpand, onRecruit, onBuildShip, onSetPendingCivilianHire, onBuildFreightCar,
  onSetChainTarget, onSetPendingTraining,
}: Props) {
  const { buildings, warehouse, labor, production_forecast, chain_targets, can_expand,
    pending_civilian_hires, pending_training, training_costs } = industry;
  const treasury = buildable?.treasury ?? 0;
  const arms = buildable?.arms ?? 0;
  const totalLabor = labor.total_labor_units;

  // Aggregate committed resources from all chain forecasts
  const committed: Record<string, number> = {};
  const pf = production_forecast;
  const addCommit = (key: string, v: number | undefined) => {
    if (v) committed[key] = (committed[key] ?? 0) + v;
  };
  addCommit('Timber', pf.timber_chain.mill_committed_timber);
  addCommit('Coal', pf.metal_chain.mill_committed_coal);
  addCommit('Iron', pf.metal_chain.mill_committed_iron);
  addCommit('Cotton', pf.textile_chain.mill_committed_cotton);
  addCommit('Wool', pf.textile_chain.mill_committed_wool);
  addCommit('Lumber', pf.timber_chain.factory_committed_lumber);
  addCommit('Steel', pf.metal_chain.factory_committed_steel);
  addCommit('Fabric', pf.textile_chain.factory_committed_fabric);

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
            {CHAIN_CONFIG.map(cfg => {
              const forecast = pf[cfg.key];
              return (
                <div key={cfg.key} style={{ marginBottom: 10 }}>
                  <div style={{ fontWeight: 'bold', color: '#daa520', marginBottom: 4 }}>
                    {cfg.emoji} {cfg.name}
                  </div>
                  <ChainOutputRow
                    label={cfg.mill.label}
                    cap={forecast.mill_cap}
                    target={chain_targets[cfg.mill.millKey]}
                    output={forecast.mill_output}
                    labor={forecast.mill_labor}
                    forecast={forecast}
                    step="mill"
                    onTargetChange={v => onSetChainTarget(cfg.chain, 'mill', v)}
                  />
                  <ChainOutputRow
                    label={cfg.factory.label}
                    cap={forecast.factory_cap}
                    target={chain_targets[cfg.factory.factoryKey]}
                    output={forecast.factory_output}
                    labor={forecast.factory_labor}
                    forecast={forecast}
                    step="factory"
                    onTargetChange={v => onSetChainTarget(cfg.chain, 'factory', v)}
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

          <EducationSection
            labor={labor}
            pendingTraining={pending_training}
            trainingCosts={training_costs}
            paper={warehouse.materials['Paper'] ?? 0}
            onSet={onSetPendingTraining}
          />
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
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '0 12px' }}>
              <div>
                <div style={{ fontSize: 9, color: '#888', textTransform: 'uppercase', marginBottom: 3 }}>Resources</div>
                {Object.entries(warehouse.resources)
                  .filter(([, v]) => v > 0)
                  .map(([k, v]) => {
                    const c = committed[k] ?? 0;
                    const free = v - c;
                    return (
                      <div key={k} style={{ fontSize: 'var(--ui-font-size, 14px)', marginBottom: 1 }}>
                        {RESOURCE_EMOJI[k] ?? '📦'} {k.replace(/([A-Z])/g, ' $1').trim()}:{' '}
                        <span style={{ color: free < v ? '#6ab0d4' : '#daa520' }}>{free}</span>
                        {c > 0 && <span style={{ fontSize: 9, color: '#555', marginLeft: 2 }}>({v})</span>}
                      </div>
                    );
                  })}
              </div>
              <div>
                <div style={{ fontSize: 9, color: '#888', textTransform: 'uppercase', marginBottom: 3 }}>Materials</div>
                {Object.entries(warehouse.materials)
                  .filter(([, v]) => v > 0)
                  .map(([k, v]) => {
                    const c = committed[k] ?? 0;
                    const free = v - c;
                    return (
                      <div key={k} style={{ fontSize: 'var(--ui-font-size, 14px)', marginBottom: 1 }}>
                        {RESOURCE_EMOJI[k] ?? '📦'} {k.replace(/([A-Z])/g, ' $1').trim()}:{' '}
                        <span style={{ color: free < v ? '#6ab0d4' : '#daa520' }}>{free}</span>
                        {c > 0 && <span style={{ fontSize: 9, color: '#555', marginLeft: 2 }}>({v})</span>}
                      </div>
                    );
                  })}
              </div>
            </div>
            {Object.values(warehouse.goods).some(v => v > 0) && (
              <div style={{ marginTop: 6 }}>
                <div style={{ fontSize: 9, color: '#888', textTransform: 'uppercase', marginBottom: 3 }}>Goods</div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '2px 8px' }}>
                  {Object.entries(warehouse.goods).filter(([, v]) => v > 0).map(([k, v]) => (
                    <span key={k} style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
                      {RESOURCE_EMOJI[k] ?? '📦'} {k.replace(/([A-Z])/g, ' $1').trim()}: <span style={{ color: '#daa520' }}>{v}</span>
                    </span>
                  ))}
                </div>
              </div>
            )}
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
              {buildable.civilians.filter(b => b.tech_met).map(b => {
                const maxCount = b.max_count ?? 0;
                const pending = pending_civilian_hires[b.type] ?? 0;
                return (
                  <CivilianHireRow
                    key={b.type}
                    emoji={CIV_EMOJI[b.type] ?? '👷'}
                    label={b.type}
                    cost={b.cost ?? 0}
                    expertRequired={b.expert_required ?? false}
                    maxCount={maxCount}
                    pending={pending}
                    onSetCount={count => onSetPendingCivilianHire(b.type, count)}
                  />
                );
              })}
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

function millInputSummary(forecast: ChainForecast, step: 'mill' | 'factory'): string {
  const parts: string[] = [];
  if (step === 'mill') {
    if (forecast.mill_committed_timber) parts.push(`${forecast.mill_committed_timber} Timber`);
    if (forecast.mill_committed_coal) parts.push(`${forecast.mill_committed_coal} Coal`);
    if (forecast.mill_committed_iron) parts.push(`${forecast.mill_committed_iron} Iron`);
    if (forecast.mill_committed_cotton) parts.push(`${forecast.mill_committed_cotton} Cotton`);
    if (forecast.mill_committed_wool) parts.push(`${forecast.mill_committed_wool} Wool`);
    const labor = forecast.mill_labor;
    if (labor > 0) parts.push(`${labor}⚒`);
    return parts.join(' + ');
  } else {
    if (forecast.factory_committed_lumber) parts.push(`${forecast.factory_committed_lumber} Lumber`);
    if (forecast.factory_committed_steel) parts.push(`${forecast.factory_committed_steel} Steel`);
    if (forecast.factory_committed_fabric) parts.push(`${forecast.factory_committed_fabric} Fabric`);
    const labor = forecast.factory_labor;
    if (labor > 0) parts.push(`${labor}⚒`);
    return parts.join(' + ');
  }
}

function ChainOutputRow({
  label, cap, target, output, labor, forecast, step, onTargetChange,
}: {
  label: string;
  cap: number;
  target: number;
  output: number;
  labor: number;
  forecast: ChainForecast;
  step: 'mill' | 'factory';
  onTargetChange: (v: number) => void;
}) {
  const [pending, setPending] = useState<number | null>(null);

  const sliderMax = cap > 0 ? cap : 10;
  // Map CHAIN_TARGET_UNLIMITED sentinel → slider max position
  const displayValue = pending !== null ? pending : (target >= CHAIN_TARGET_UNLIMITED ? sliderMax : Math.min(target, sliderMax));

  const commit = useCallback(() => {
    if (pending !== null) {
      // slider at max → send unlimited
      const actual = pending >= sliderMax ? CHAIN_TARGET_UNLIMITED : pending;
      onTargetChange(actual);
      setPending(null);
    }
  }, [pending, sliderMax, onTargetChange]);

  const inputSummary = millInputSummary(forecast, step);
  const outputLabel = output > 0 ? `→ ${output}` : '→ 0';

  return (
    <div style={{ marginBottom: 8, paddingLeft: 4 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, color: '#bbb', marginBottom: 2 }}>
        <span>{label}</span>
        <span style={{ color: output > 0 ? '#daa520' : '#666', flexShrink: 0, marginLeft: 4 }}>
          {inputSummary ? `${inputSummary} ${outputLabel}` : outputLabel}
        </span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <span style={{ fontSize: 9, color: '#888', width: 22, flexShrink: 0 }}>🏭</span>
        <input
          type="range" min={0} max={sliderMax} step={1}
          value={displayValue}
          onChange={e => setPending(Number(e.target.value))}
          onMouseUp={commit}
          onTouchEnd={commit}
          style={{ flex: 1, accentColor: '#daa520', height: 4, cursor: 'pointer' }}
        />
        <span style={{ fontSize: 9, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
          {displayValue >= sliderMax ? '∞' : String(displayValue)}
        </span>
      </div>
    </div>
  );
}

function EducationSection({
  labor, pendingTraining, trainingCosts, paper, onSet,
}: {
  labor: IndustryData['labor'];
  pendingTraining: { to_trained: number; to_expert: number };
  trainingCosts: IndustryData['training_costs'];
  paper: number;
  onSet: (toTrained: number, toExpert: number) => void;
}) {
  const [pendToTrained, setPendToTrained] = useState<number | null>(null);
  const [pendToExpert, setPendToExpert] = useState<number | null>(null);

  const toTrainedVal = pendToTrained ?? pendingTraining.to_trained;
  const toExpertVal = pendToExpert ?? pendingTraining.to_expert;

  const maxToTrained = labor.untrained;
  const maxToExpert = labor.trained;

  const commitBoth = useCallback((tt: number, te: number) => {
    onSet(tt, te);
    setPendToTrained(null);
    setPendToExpert(null);
  }, [onSet]);

  return (
    <Section label="Education" emoji="🎓">
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, color: '#bbb', marginBottom: 2 }}>
            <span>👕→👔 Untrained→Trained</span>
            <span style={{ color: '#888' }}>
              cost: {trainingCosts.to_trained_paper}📄 + {trainingCosts.to_trained_labor}⚒ each
            </span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <input
              type="range" min={0} max={maxToTrained} step={1}
              value={toTrainedVal}
              onChange={e => setPendToTrained(Number(e.target.value))}
              onMouseUp={() => commitBoth(toTrainedVal, toExpertVal)}
              onTouchEnd={() => commitBoth(toTrainedVal, toExpertVal)}
              style={{ flex: 1, accentColor: '#6ab0d4', height: 4, cursor: 'pointer' }}
            />
            <span style={{ fontSize: 10, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
              {toTrainedVal}
            </span>
          </div>
          {toTrainedVal > 0 && (
            <div style={{ fontSize: 9, color: '#888', marginTop: 1 }}>
              Cost: {toTrainedVal * trainingCosts.to_trained_paper}📄 + {toTrainedVal * trainingCosts.to_trained_labor}⚒
              {paper < toTrainedVal * trainingCosts.to_trained_paper && (
                <span style={{ color: '#a66', marginLeft: 4 }}>⚠ need more paper</span>
              )}
            </div>
          )}
        </div>

        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, color: '#bbb', marginBottom: 2 }}>
            <span>👔→🥼 Trained→Expert</span>
            <span style={{ color: '#888' }}>
              cost: {trainingCosts.to_expert_paper}📄 + {trainingCosts.to_expert_labor}⚒ each
            </span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <input
              type="range" min={0} max={maxToExpert} step={1}
              value={toExpertVal}
              onChange={e => setPendToExpert(Number(e.target.value))}
              onMouseUp={() => commitBoth(toTrainedVal, toExpertVal)}
              onTouchEnd={() => commitBoth(toTrainedVal, toExpertVal)}
              style={{ flex: 1, accentColor: '#4a8fd4', height: 4, cursor: 'pointer' }}
            />
            <span style={{ fontSize: 10, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
              {toExpertVal}
            </span>
          </div>
          {toExpertVal > 0 && (
            <div style={{ fontSize: 9, color: '#888', marginTop: 1 }}>
              Cost: {toExpertVal * trainingCosts.to_expert_paper}📄 + {toExpertVal * trainingCosts.to_expert_labor}⚒
            </div>
          )}
        </div>
      </div>
    </Section>
  );
}

function CivilianHireRow({
  emoji, label, cost, expertRequired, maxCount, pending, onSetCount,
}: {
  emoji: string;
  label: string;
  cost: number;
  expertRequired: boolean;
  maxCount: number;
  pending: number;
  onSetCount: (count: number) => void;
}) {
  const [localVal, setLocalVal] = useState<number | null>(null);
  const display = localVal ?? pending;

  const commit = useCallback(() => {
    if (localVal !== null) {
      onSetCount(localVal);
      setLocalVal(null);
    }
  }, [localVal, onSetCount]);

  const canHire = maxCount > 0;
  const sublabel = expertRequired ? `$${cost} + 1 expert` : `$${cost}`;

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 0', opacity: canHire ? 1 : 0.45 }}>
      <span style={{ width: 18, textAlign: 'center' }}>{emoji}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ fontSize: 'var(--ui-font-size, 14px)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {label}
          </span>
          <span style={{ fontSize: 10, color: '#888', marginLeft: 4, flexShrink: 0 }}>{sublabel}</span>
        </div>
        {canHire ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 2 }}>
            <input
              type="range" min={0} max={maxCount} step={1}
              value={display}
              onChange={e => setLocalVal(Number(e.target.value))}
              onMouseUp={commit}
              onTouchEnd={commit}
              style={{ flex: 1, cursor: 'pointer', accentColor: '#daa520', height: 4 }}
            />
            <span style={{ fontSize: 10, color: display > 0 ? '#daa520' : '#555', width: 24, textAlign: 'right', flexShrink: 0 }}>
              {display > 0 ? `+${display}` : ''}
            </span>
          </div>
        ) : (
          <div style={{ fontSize: 10, color: '#a66', marginTop: 1 }}>
            {expertRequired ? 'Need expert workers' : 'Cannot afford'}
          </div>
        )}
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

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setValue(Number(e.target.value));
  }, []);

  const handleCommit = useCallback(() => {
    if (value > 0 && canAfford) {
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

function fmtName(s: string) {
  return s.replace(/([A-Z])/g, ' $1').trim();
}

function btnStyle(bg: string): React.CSSProperties {
  return { background: bg, color: '#fff', border: 'none', borderRadius: 3, padding: '1px 6px', fontSize: 10, cursor: 'pointer' };
}
