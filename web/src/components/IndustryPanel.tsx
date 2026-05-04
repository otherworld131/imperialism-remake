import { useState, useCallback, useRef, useEffect } from 'react';
import type { IndustryData, BuildableUnits, ChainForecast, ArmsChainForecast, PaperChainForecast } from '../wasm';

interface Props {
  industry: IndustryData;
  buildable: BuildableUnits | null;
  onExpand: (buildingType: string) => void;
  onSetPendingArmyRecruit: (unitType: string, count: number) => void;
  onSetPendingShips: (shipType: string, count: number) => void;
  onSetPendingCivilianHire: (civilianType: string, count: number) => void;
  onSetPendingFreightCars: (count: number) => void;
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
    arms: true,
  },
  {
    key: 'textile_chain' as const,
    chain: 'textile',
    name: 'Textile', emoji: '🧵',
    mill: { label: 'Cotton/Wool → Fabric', millKey: 'textile_mill' as const },
    factory: { label: 'Fabric → Clothing', factoryKey: 'garment_factory' as const },
  },
] as const;

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
  onExpand, onSetPendingArmyRecruit, onSetPendingShips, onSetPendingCivilianHire, onSetPendingFreightCars,
  onSetChainTarget, onSetPendingTraining,
}: Props) {
  const { buildings, warehouse, labor, production_forecast, chain_targets, can_expand,
    pending_civilian_hires, pending_training, training_costs,
    pending_freight_cars, max_freight_cars, pending_ships, pending_army_recruits,
    army_committed_arms, army_committed_horses } = industry;
  const treasury = buildable?.treasury ?? 0;
  const arms = buildable?.arms ?? 0;
  const pf = production_forecast;

  // Aggregate committed resources + materials from all chain forecasts
  const committed: Record<string, number> = {};
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
  if (pf.arms_chain.armory_committed_steel) {
    committed['Steel'] = (committed['Steel'] ?? 0) + pf.arms_chain.armory_committed_steel;
  }
  // Paper chain: deduct lumber committed to paper production
  if (pf.paper_chain?.factory_committed_lumber) {
    committed['Lumber'] = (committed['Lumber'] ?? 0) + pf.paper_chain.factory_committed_lumber;
  }
  // Education: deduct paper committed to training
  const paperPerTrained = training_costs.to_trained_paper;
  const paperPerExpert = training_costs.to_expert_paper;
  const committedPaper = (pending_training.to_trained * paperPerTrained) + (pending_training.to_expert * paperPerExpert);
  if (committedPaper > 0) addCommit('Paper', committedPaper);
  // Freight cars: deduct committed lumber + steel (1 each per car)
  const [fcLaborPerCar, fcLumberPerCar, fcSteelPerCar] = [2, 1, 1];
  if (pending_freight_cars > 0) {
    addCommit('Lumber', pending_freight_cars * fcLumberPerCar);
    addCommit('Steel', pending_freight_cars * fcSteelPerCar);
  }
  // Army recruits: deduct arms and horses committed by the queue
  addCommit('Arms', army_committed_arms);
  addCommit('Horses', army_committed_horses);

  // Production labor committed (sum of all forecast labor)
  const productionLaborCommitted =
    (pf.timber_chain.mill_labor ?? 0) + (pf.timber_chain.factory_labor ?? 0) +
    (pf.metal_chain.mill_labor ?? 0) + (pf.metal_chain.factory_labor ?? 0) +
    (pf.textile_chain.mill_labor ?? 0) + (pf.textile_chain.factory_labor ?? 0) +
    (pf.arms_chain.armory_labor ?? 0) +
    (pf.paper_chain?.factory_labor ?? 0);

  const totalLaborUnits = labor.total_labor_units;
  const committedLaborUnits = (labor.committed_labor_units ?? 0) + productionLaborCommitted;
  const freeLabor = totalLaborUnits - committedLaborUnits;

  return (
    <div style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
      {/* Treasury & Arms header */}
      <div style={{ display: 'flex', gap: 12, marginBottom: 8, color: '#daa520', fontWeight: 'bold' }}>
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
              const showArms = (cfg as { arms?: boolean }).arms && pf.arms_chain.armory_cap > 0;
              const showPaper = cfg.chain === 'timber' && pf.paper_chain?.factory_cap > 0;
              return (
                <div key={cfg.key} style={{ marginBottom: 10 }}>
                  <div style={{ fontWeight: 'bold', color: '#daa520', marginBottom: 4 }}>
                    {cfg.emoji} {cfg.name}
                  </div>
                  <ChainOutputRow
                    label={cfg.mill.label}
                    cap={forecast.mill_cap}
                    maxOutput={forecast.mill_max_output ?? forecast.mill_cap}
                    target={chain_targets[cfg.mill.millKey]}
                    output={forecast.mill_output}
                    forecast={forecast}
                    step="mill"
                    onTargetChange={v => onSetChainTarget(cfg.chain, 'mill', v)}
                  />
                  <ChainOutputRow
                    label={cfg.factory.label}
                    cap={forecast.factory_cap}
                    maxOutput={forecast.factory_max_output ?? forecast.factory_cap}
                    target={chain_targets[cfg.factory.factoryKey]}
                    output={forecast.factory_output}
                    forecast={forecast}
                    step="factory"
                    onTargetChange={v => onSetChainTarget(cfg.chain, 'factory', v)}
                  />
                  {showPaper && (
                    <PaperRow
                      forecast={pf.paper_chain}
                      target={chain_targets.paper_factory ?? 0}
                      label="Timber → Paper"
                      onTargetChange={v => onSetChainTarget('timber', 'paper', v)}
                    />
                  )}
                  {showArms && (
                    <ArmoryRow
                      cap={pf.arms_chain.armory_cap}
                      maxOutput={pf.arms_chain.armory_max_output}
                      target={chain_targets.armory}
                      output={pf.arms_chain.armory_output}
                      committedSteel={pf.arms_chain.armory_committed_steel}
                      labor={pf.arms_chain.armory_labor}
                      onTargetChange={v => onSetChainTarget('arms', 'armory', v)}
                    />
                  )}
                </div>
              );
            })}
          </Section>

          <Section label="Labor" emoji="👷">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <LaborRow emoji="👕" label="Untrained" count={labor.untrained} committed={labor.committed_untrained ?? 0} color="#888" />
              <LaborRow emoji="👔" label="Trained" count={labor.trained} committed={labor.committed_trained ?? 0} color="#6ab0d4" />
              <LaborRow emoji="🥼" label="Expert" count={labor.expert} committed={labor.committed_expert ?? 0} color="#4a8fd4" />
              <div style={{ borderTop: '1px solid #333', paddingTop: 4, marginTop: 2, color: '#daa520', fontSize: 11 }}>
                = {committedLaborUnits > 0
                  ? <><span style={{ color: '#6ab0d4' }}>{Math.max(0, freeLabor)}</span><span style={{ fontSize: 9, color: '#555' }}> ({totalLaborUnits})</span></>
                  : totalLaborUnits
                } labor units
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
            <FreightCarRow
              pending={pending_freight_cars}
              max={max_freight_cars}
              onSetCount={onSetPendingFreightCars}
            />
          </Section>

          {buildable && buildable.army.filter(b => b.tech_met).length > 0 && (
            <Section label="Army Recruitment" emoji="⚔️">
              {buildable.army.filter(b => b.tech_met).map(b => {
                const queued = (pending_army_recruits ?? []).filter(s => s === b.type).length;
                const maxCount = b.max_count ?? 0;
                return (
                  <ShipBuildRow
                    key={b.type}
                    emoji={UNIT_EMOJI[b.category ?? ''] ?? '⚔️'}
                    label={fmtName(b.type)}
                    sublabel={`$${b.cost}${b.arms_required ? ` +${b.arms_required}A` : ''}`}
                    maxCount={maxCount}
                    queued={queued}
                    techMet={b.tech_met}
                    reason={maxCount === 0 ? (b.reason ?? 'Cannot recruit') : null}
                    onSetCount={count => onSetPendingArmyRecruit(b.type, count)}
                  />
                );
              })}
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
                    {ships.map(b => {
                      const queued = (pending_ships ?? []).filter(s => s === b.type).length;
                      const maxCount = b.max_count ?? 0;
                      return (
                        <ShipBuildRow
                          key={b.type}
                          emoji={SHIP_EMOJI[cat] ?? '⚓'}
                          label={fmtName(b.type)}
                          sublabel={b.resources_needed
                            ? Object.entries(b.resources_needed).map(([k, v]) => `${v}${k[0].toUpperCase()}`).join('+')
                            : ''}
                          maxCount={maxCount}
                          queued={queued}
                          techMet={b.tech_met}
                          reason={!b.tech_met ? (b.reason ?? 'Tech required') : (maxCount === 0 ? 'Insufficient resources' : null)}
                          onSetCount={count => onSetPendingShips(b.type, count)}
                        />
                      );
                    })}
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

function clampTarget(target: number, cap: number): number {
  // Any value at or above CHAIN_TARGET_UNLIMITED sentinel (u32::MAX) maps to cap
  return target >= 4294967295 ? cap : Math.max(0, Math.min(target, cap));
}

function ChainOutputRow({
  label, cap, maxOutput, target, output, forecast, step, onTargetChange,
}: {
  label: string;
  cap: number;
  maxOutput: number;
  target: number;
  output: number;
  forecast: ChainForecast;
  step: 'mill' | 'factory';
  onTargetChange: (v: number) => void;
}) {
  const effectiveCap = Math.max(0, Math.min(cap, maxOutput));
  const [localValue, setLocalValue] = useState<number>(() => clampTarget(target, effectiveCap));
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<number | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null && target !== lastSentRef.current) return;
    lastSentRef.current = null;
    setLocalValue(clampTarget(target, effectiveCap));
  }, [target, effectiveCap]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    isDraggingRef.current = true;
    setLocalValue(Number(e.target.value));
  }, []);

  const commit = useCallback(() => {
    isDraggingRef.current = false;
    lastSentRef.current = localValue;
    onTargetChange(localValue);
  }, [localValue, onTargetChange]);

  if (cap === 0) {
    return (
      <div style={{ marginBottom: 8, paddingLeft: 4 }}>
        <div style={{ fontSize: 11, color: '#555' }}>{label}</div>
        <div style={{ fontSize: 10, color: '#444', fontStyle: 'italic' }}>No building</div>
      </div>
    );
  }

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
          type="range" min={0} max={effectiveCap} step={1}
          value={localValue}
          onChange={handleChange}
          onMouseUp={commit}
          onTouchEnd={commit}
          style={{ flex: 1, accentColor: '#daa520', height: 4, cursor: 'pointer' }}
        />
        <span style={{ fontSize: 9, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
          {localValue}/{effectiveCap}
        </span>
      </div>
    </div>
  );
}

function ArmoryRow({
  cap, maxOutput, target, output, committedSteel, labor, onTargetChange,
}: {
  cap: number;
  maxOutput: number;
  target: number;
  output: number;
  committedSteel: number;
  labor: number;
  onTargetChange: (v: number) => void;
}) {
  const effectiveCap = Math.max(0, Math.min(cap, maxOutput));
  const [localValue, setLocalValue] = useState<number>(() => clampTarget(target, effectiveCap));
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<number | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null && target !== lastSentRef.current) return;
    lastSentRef.current = null;
    setLocalValue(clampTarget(target, effectiveCap));
  }, [target, effectiveCap]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    isDraggingRef.current = true;
    setLocalValue(Number(e.target.value));
  }, []);

  const commit = useCallback(() => {
    isDraggingRef.current = false;
    lastSentRef.current = localValue;
    onTargetChange(localValue);
  }, [localValue, onTargetChange]);

  const parts: string[] = [];
  if (committedSteel > 0) parts.push(`${committedSteel} Steel`);
  if (labor > 0) parts.push(`${labor}⚒`);
  const inputSummary = parts.join(' + ');
  const outputLabel = output > 0 ? `→ ${output}` : '→ 0';

  return (
    <div style={{ marginBottom: 8, paddingLeft: 4 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, color: '#bbb', marginBottom: 2 }}>
        <span>Steel → Arms</span>
        <span style={{ color: output > 0 ? '#daa520' : '#666', flexShrink: 0, marginLeft: 4 }}>
          {inputSummary ? `${inputSummary} ${outputLabel}` : outputLabel}
        </span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <span style={{ fontSize: 9, color: '#888', width: 22, flexShrink: 0 }}>🗡️</span>
        <input
          type="range" min={0} max={effectiveCap} step={1}
          value={localValue}
          onChange={handleChange}
          onMouseUp={commit}
          onTouchEnd={commit}
          style={{ flex: 1, accentColor: '#daa520', height: 4, cursor: 'pointer' }}
        />
        <span style={{ fontSize: 9, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
          {localValue}/{effectiveCap}
        </span>
      </div>
    </div>
  );
}

function PaperRow({
  forecast, target, label, onTargetChange,
}: {
  forecast: PaperChainForecast;
  target: number;
  label: string;
  onTargetChange: (v: number) => void;
}) {
  const effectiveCap = Math.max(0, Math.min(forecast.factory_cap, forecast.factory_max_output));
  const [localValue, setLocalValue] = useState<number>(() => clampTarget(target, effectiveCap));
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<number | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null && target !== lastSentRef.current) return;
    lastSentRef.current = null;
    setLocalValue(clampTarget(target, effectiveCap));
  }, [target, effectiveCap]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    isDraggingRef.current = true;
    setLocalValue(Number(e.target.value));
  }, []);

  const commit = useCallback(() => {
    isDraggingRef.current = false;
    lastSentRef.current = localValue;
    onTargetChange(localValue);
  }, [localValue, onTargetChange]);

  const parts: string[] = [];
  if (forecast.factory_committed_lumber > 0) parts.push(`${forecast.factory_committed_lumber} Lumber`);
  if (forecast.factory_labor > 0) parts.push(`${forecast.factory_labor}⚒`);
  const inputSummary = parts.join(' + ');
  const outputLabel = forecast.factory_output > 0 ? `→ ${forecast.factory_output}` : '→ 0';

  return (
    <div style={{ marginBottom: 8, paddingLeft: 4 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, color: '#bbb', marginBottom: 2 }}>
        <span>{label}</span>
        <span style={{ color: forecast.factory_output > 0 ? '#daa520' : '#666', flexShrink: 0, marginLeft: 4 }}>
          {inputSummary ? `${inputSummary} ${outputLabel}` : outputLabel}
        </span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <span style={{ fontSize: 9, color: '#888', width: 22, flexShrink: 0 }}>📄</span>
        <input
          type="range" min={0} max={effectiveCap} step={1}
          value={localValue}
          onChange={handleChange}
          onMouseUp={commit}
          onTouchEnd={commit}
          style={{ flex: 1, accentColor: '#daa520', height: 4, cursor: 'pointer' }}
        />
        <span style={{ fontSize: 9, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
          {localValue}/{effectiveCap}
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
  const [toTrainedLocal, setToTrainedLocal] = useState<number>(pendingTraining.to_trained);
  const [toExpertLocal, setToExpertLocal] = useState<number>(pendingTraining.to_expert);
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<{ tt: number; te: number } | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null) {
      if (pendingTraining.to_trained === lastSentRef.current.tt &&
          pendingTraining.to_expert === lastSentRef.current.te) {
        lastSentRef.current = null;
      } else {
        return;
      }
    }
    setToTrainedLocal(pendingTraining.to_trained);
    setToExpertLocal(pendingTraining.to_expert);
  }, [pendingTraining.to_trained, pendingTraining.to_expert]);

  // Cap to_trained by: untrained workers AND paper available / cost
  const maxToTrainedByWorkers = labor.untrained;
  const maxToTrainedByPaper = trainingCosts.to_trained_paper > 0
    ? Math.floor(paper / trainingCosts.to_trained_paper)
    : labor.untrained;
  const maxToTrained = Math.min(maxToTrainedByWorkers, maxToTrainedByPaper);
  const maxToExpert = labor.trained;

  const commitBoth = useCallback((tt: number, te: number) => {
    isDraggingRef.current = false;
    lastSentRef.current = { tt, te };
    onSet(tt, te);
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
              value={toTrainedLocal}
              onChange={e => { isDraggingRef.current = true; setToTrainedLocal(Number(e.target.value)); }}
              onMouseUp={() => commitBoth(toTrainedLocal, toExpertLocal)}
              onTouchEnd={() => commitBoth(toTrainedLocal, toExpertLocal)}
              style={{ flex: 1, accentColor: '#6ab0d4', height: 4, cursor: 'pointer' }}
            />
            <span style={{ fontSize: 10, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
              {toTrainedLocal}
            </span>
          </div>
          {toTrainedLocal > 0 && (
            <div style={{ fontSize: 9, color: '#888', marginTop: 1 }}>
              Cost: {toTrainedLocal * trainingCosts.to_trained_paper}📄 + {toTrainedLocal * trainingCosts.to_trained_labor}⚒
              {paper < toTrainedLocal * trainingCosts.to_trained_paper && (
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
              value={toExpertLocal}
              onChange={e => { isDraggingRef.current = true; setToExpertLocal(Number(e.target.value)); }}
              onMouseUp={() => commitBoth(toTrainedLocal, toExpertLocal)}
              onTouchEnd={() => commitBoth(toTrainedLocal, toExpertLocal)}
              style={{ flex: 1, accentColor: '#4a8fd4', height: 4, cursor: 'pointer' }}
            />
            <span style={{ fontSize: 10, color: '#aaa', width: 28, textAlign: 'right', flexShrink: 0 }}>
              {toExpertLocal}
            </span>
          </div>
          {toExpertLocal > 0 && (
            <div style={{ fontSize: 9, color: '#888', marginTop: 1 }}>
              Cost: {toExpertLocal * trainingCosts.to_expert_paper}📄 + {toExpertLocal * trainingCosts.to_expert_labor}⚒
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
  const [localVal, setLocalVal] = useState<number>(pending);
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<number | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null && pending !== lastSentRef.current) return;
    lastSentRef.current = null;
    setLocalVal(pending);
  }, [pending]);

  const commit = useCallback(() => {
    isDraggingRef.current = false;
    lastSentRef.current = localVal;
    onSetCount(localVal);
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
              value={localVal}
              onChange={e => { isDraggingRef.current = true; setLocalVal(Number(e.target.value)); }}
              onMouseUp={commit}
              onTouchEnd={commit}
              style={{ flex: 1, cursor: 'pointer', accentColor: '#daa520', height: 4 }}
            />
            <span style={{ fontSize: 10, color: localVal > 0 ? '#daa520' : '#555', width: 24, textAlign: 'right', flexShrink: 0 }}>
              {localVal > 0 ? `+${localVal}` : ''}
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

function FreightCarRow({
  pending, max, onSetCount,
}: {
  pending: number;
  max: number;
  onSetCount: (count: number) => void;
}) {
  const [localVal, setLocalVal] = useState<number>(pending);
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<number | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null && pending !== lastSentRef.current) return;
    lastSentRef.current = null;
    setLocalVal(pending);
  }, [pending]);

  const commit = useCallback(() => {
    isDraggingRef.current = false;
    lastSentRef.current = localVal;
    onSetCount(localVal);
  }, [localVal, onSetCount]);

  if (max === 0) {
    return (
      <div style={{ fontSize: 10, color: '#555', fontStyle: 'italic' }}>Cannot build (need lumber + steel + labor)</div>
    );
  }

  return (
    <div style={{ padding: '3px 0' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, color: '#bbb', marginBottom: 2 }}>
        <span>🚃 Freight Cars</span>
        <span style={{ fontSize: 10, color: '#888' }}>1L + 1S + 2⚒ each</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <input
          type="range" min={0} max={max} step={1}
          value={localVal}
          onChange={e => { isDraggingRef.current = true; setLocalVal(Number(e.target.value)); }}
          onMouseUp={commit}
          onTouchEnd={commit}
          style={{ flex: 1, cursor: 'pointer', accentColor: '#daa520', height: 4 }}
        />
        <span style={{ fontSize: 10, color: localVal > 0 ? '#daa520' : '#555', width: 28, textAlign: 'right', flexShrink: 0 }}>
          {localVal > 0 ? `+${localVal}` : '0'}
        </span>
      </div>
    </div>
  );
}

function LaborRow({ emoji, label, count, committed, color }: { emoji: string; label: string; count: number; committed: number; color: string }) {
  const free = count - committed;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
      <span style={{ fontSize: 16, lineHeight: 1 }}>{emoji}</span>
      <span style={{ color: '#aaa', flex: 1 }}>{label}</span>
      <span style={{ color: committed > 0 ? '#6ab0d4' : color, fontWeight: 'bold' }}>{free}</span>
      {committed > 0 && <span style={{ fontSize: 9, color: '#555' }}>({count})</span>}
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

function ShipBuildRow({
  emoji, label, sublabel, maxCount, queued, techMet, reason, onSetCount,
}: {
  emoji: string;
  label: string;
  sublabel: string;
  maxCount: number;
  queued: number;
  techMet: boolean;
  reason: string | null;
  onSetCount: (count: number) => void;
}) {
  const [localVal, setLocalVal] = useState<number>(queued);
  const isDraggingRef = useRef(false);
  const lastSentRef = useRef<number | null>(null);

  useEffect(() => {
    if (isDraggingRef.current) return;
    if (lastSentRef.current !== null && queued !== lastSentRef.current) return;
    lastSentRef.current = null;
    setLocalVal(queued);
  }, [queued]);

  const commit = useCallback(() => {
    isDraggingRef.current = false;
    lastSentRef.current = localVal;
    onSetCount(localVal);
  }, [localVal, onSetCount]);

  const canBuild = techMet && maxCount > 0;
  const showSlider = canBuild || queued > 0;
  const sliderMax = Math.max(maxCount, queued);

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 0', opacity: (canBuild || queued > 0) ? 1 : 0.45 }}>
      <span style={{ width: 18, textAlign: 'center' }}>{emoji}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ fontSize: 'var(--ui-font-size, 14px)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {label}
          </span>
          <span style={{ fontSize: 10, color: '#888', marginLeft: 4, flexShrink: 0 }}>{sublabel}</span>
        </div>
        {showSlider ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 2 }}>
            <input
              type="range" min={0} max={sliderMax} step={1}
              value={localVal}
              onChange={e => { isDraggingRef.current = true; setLocalVal(Number(e.target.value)); }}
              onMouseUp={commit}
              onTouchEnd={commit}
              style={{ flex: 1, cursor: 'pointer', accentColor: '#daa520', height: 4 }}
            />
            <span style={{ fontSize: 10, color: localVal > 0 ? '#daa520' : '#555', width: 28, textAlign: 'right', flexShrink: 0 }}>
              {localVal > 0 ? `+${localVal}` : '0'}
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
