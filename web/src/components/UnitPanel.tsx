import type { ProvinceUnits, PendingMove } from '../wasm';
import { HealthBar } from './UnitRow';

const CATEGORY_ICONS: Record<string, string> = {
  Infantry: '\u2694\uFE0F',   // ⚔️
  Cavalry: '\u{1F40E}',       // 🐎
  Artillery: '\u{1F4A3}',     // 💣
  Special: '\u2B50',           // ⭐
  Garrison: '\u{1F6E1}\uFE0F', // 🛡️
};

interface Props {
  provinceUnits: ProvinceUnits;
  pendingMoves: PendingMove[];
  isPlayerProvince: boolean;
  selectedUnitIds: number[];
  onToggleUnit: (unitId: number) => void;
  onSelectAll: () => void;
  onCancelMove: (unitId: number) => void;
  onCancelSelectedMoves: () => void;
  onDismissSelected: () => void;
  onUpgradeUnit: (unitId: number) => void;
  onUpgradeSelected: () => void;
  showHealDebug?: boolean;
}

const HEAL_BLOCK_LABEL: Record<string, string> = {
  moved: 'no heal — moved this turn',
  fought: 'no heal — fought this turn',
  full_health: 'no heal — already at full HP',
};

export default function UnitPanel({
  provinceUnits, pendingMoves,
  isPlayerProvince,
  selectedUnitIds, onToggleUnit, onSelectAll,
  onCancelMove, onCancelSelectedMoves, onDismissSelected,
  onUpgradeUnit, onUpgradeSelected,
  showHealDebug,
}: Props) {
  const { army_units, garrison_count, province_name } = provinceUnits;

  const selectableUnits = army_units.filter(u => u.category !== 'Garrison');
  const hasSelection = selectedUnitIds.length > 0;
  const selectedWithPendingMove = selectedUnitIds.filter(
    id => pendingMoves.some(m => m.unit_id === id)
  ).length;
  // Card #417: which selected units actually have an unlocked upgrade target.
  const selectedUpgradable = army_units.filter(
    u => selectedUnitIds.includes(u.id) && u.upgrade_to,
  );

  return (
    <div style={{ fontSize: 13 }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6 }}>{province_name}</div>

      {garrison_count > 0 && (
        <div style={{ color: '#aaa', marginBottom: 4 }}>
          {CATEGORY_ICONS.Garrison} Garrison: {garrison_count} militia
        </div>
      )}

      {army_units.length > 0 && (
        <>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
            <span style={{ color: '#ccc', fontWeight: 'bold' }}>
              Army Units ({army_units.length})
            </span>
            <div style={{ display: 'flex', gap: 4 }}>
              {isPlayerProvince && hasSelection && selectedWithPendingMove > 0 && (
                <button onClick={onCancelSelectedMoves} style={btnStyle('#a33')}>
                  Cancel Moves
                </button>
              )}
              {isPlayerProvince && selectedUpgradable.length > 0 && (
                <button onClick={onUpgradeSelected} style={btnStyle('#48a')}>
                  Upgrade {selectedUpgradable.length}
                </button>
              )}
              {isPlayerProvince && hasSelection && (
                <button onClick={onDismissSelected} style={btnStyle('#a33')}>
                  Dismiss
                </button>
              )}
              {isPlayerProvince && hasSelection && (
                <button onClick={onSelectAll} style={btnStyle('#456')}>
                  {selectedUnitIds.length === selectableUnits.length ? 'Deselect' : 'Select All'}
                </button>
              )}
              {isPlayerProvince && !hasSelection && selectableUnits.length > 1 && (
                <button onClick={onSelectAll} style={btnStyle('#456')}>
                  Select All
                </button>
              )}
            </div>
          </div>

          {hasSelection && isPlayerProvince && (
            <div style={{ fontSize: 10, color: '#aaa', marginBottom: 4, fontStyle: 'italic' }}>
              Click a highlighted hex to move {selectedUnitIds.length} unit{selectedUnitIds.length > 1 ? 's' : ''} {'\u00b7'} click units to toggle {'\u00b7'} Esc to cancel
            </div>
          )}

          {army_units.map(unit => {
            const pending = pendingMoves.find(m => m.unit_id === unit.id);
            const icon = CATEGORY_ICONS[unit.category] || '';
            const stars = '\u2605'.repeat(unit.medals);
            const isSelectable = unit.category !== 'Garrison';
            const isSelected = selectedUnitIds.includes(unit.id);
            return (
              <div key={unit.id} style={{
                background: isSelected ? 'rgba(218,165,32,0.15)' : 'rgba(255,255,255,0.05)',
                borderRadius: 4,
                padding: '4px 6px',
                marginBottom: 3,
                border: isSelected ? '1px solid rgba(218,165,32,0.4)' : '1px solid transparent',
                cursor: isSelectable && isPlayerProvince ? 'pointer' : 'default',
              }}
                onClick={() => {
                  if (isSelectable && isPlayerProvince) onToggleUnit(unit.id);
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <span>
                    {isSelectable && isPlayerProvince && (
                      <span style={{
                        display: 'inline-block', width: 12, height: 12,
                        border: '1px solid #888', borderRadius: 2, marginRight: 4,
                        background: isSelected ? '#daa520' : 'transparent',
                        verticalAlign: 'middle', fontSize: 9, textAlign: 'center', lineHeight: '12px',
                      }}>
                        {isSelected ? '\u2713' : ''}
                      </span>
                    )}
                    {icon} {unit.unit_type.replace(/([A-Z])/g, ' $1').trim()}
                    {stars && <span style={{ color: '#ffd700', marginLeft: 4 }}>{stars}</span>}
                  </span>
                  <span style={{ fontSize: 11, color: '#999' }}>
                    FP {unit.effective_firepower.toFixed(1)}
                  </span>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 2 }}>
                  <HealthBar health={unit.health} />
                  <span style={{ fontSize: 10, color: '#888' }}>{unit.health}%</span>
                </div>
                {showHealDebug && unit.heal_blocked_reason && (
                  <div style={{ marginTop: 2, fontSize: 10, color: '#e88', fontStyle: 'italic' }}>
                    {HEAL_BLOCK_LABEL[unit.heal_blocked_reason] ?? `no heal — ${unit.heal_blocked_reason}`}
                  </div>
                )}
                {showHealDebug && !unit.heal_blocked_reason && unit.health < 100 && (
                  <div style={{ marginTop: 2, fontSize: 10, color: '#8e8', fontStyle: 'italic' }}>
                    healed last turn
                  </div>
                )}
                {pending && (
                  <div style={{ marginTop: 3, fontSize: 11 }}>
                    <span style={{ color: '#ffd700' }}>
                      {'\u2192'} {pending.destination_name}
                    </span>
                    <button onClick={(e) => { e.stopPropagation(); onCancelMove(unit.id); }} style={{ ...btnStyle('#a33'), marginLeft: 8 }}>
                      Cancel
                    </button>
                  </div>
                )}
                {/* Card #417: per-unit Upgrade button when an unlocked target exists. */}
                {isPlayerProvince && unit.upgrade_to && (
                  <div style={{ marginTop: 3, fontSize: 11, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ color: '#7af' }}>
                      {'\u21e7'} {unit.upgrade_to.replace(/([A-Z])/g, ' $1').trim()}
                      <span style={{ color: '#888', marginLeft: 4 }}>
                        ${unit.upgrade_cost ?? 0}{unit.upgrade_arms_delta ? ` + ${unit.upgrade_arms_delta}A` : ''}
                      </span>
                    </span>
                    <button onClick={(e) => { e.stopPropagation(); onUpgradeUnit(unit.id); }} style={btnStyle('#48a')}>
                      Upgrade
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </>
      )}

      {army_units.length === 0 && garrison_count === 0 && (
        <div style={{ color: '#888', fontStyle: 'italic' }}>No units in province</div>
      )}

    </div>
  );
}

function btnStyle(bg: string): React.CSSProperties {
  return {
    background: bg, color: '#fff', border: 'none', borderRadius: 3,
    padding: '1px 6px', fontSize: 10, cursor: 'pointer',
  };
}
