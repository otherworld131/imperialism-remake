import type { ArmyUnitDetail, ProvinceUnits, BuildableUnit, PendingMove } from '../wasm';
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
  buildableArmy: BuildableUnit[];
  treasury: number;
  arms: number;
  pendingMoves: PendingMove[];
  isPlayerCapital: boolean;
  isPlayerProvince: boolean;
  selectedUnitIds: number[];
  onToggleUnit: (unitId: number, shiftKey: boolean) => void;
  onSelectAll: () => void;
  onMoveSelected: () => void;
  onMoveUnit: (unitId: number) => void;
  onCancelMove: (unitId: number) => void;
  onRecruit: (unitType: string) => void;
}

export default function UnitPanel({
  provinceUnits, buildableArmy, treasury, arms, pendingMoves,
  isPlayerCapital, isPlayerProvince,
  selectedUnitIds, onToggleUnit, onSelectAll, onMoveSelected,
  onMoveUnit, onCancelMove, onRecruit,
}: Props) {
  const { army_units, garrison_count, province_name } = provinceUnits;

  const movableUnits = army_units.filter(
    u => u.category !== 'Garrison' && !pendingMoves.some(m => m.unit_id === u.id)
  );
  const hasSelection = selectedUnitIds.length > 0;

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
            {isPlayerProvince && movableUnits.length > 1 && (
              <button onClick={onSelectAll} style={btnStyle('#456')}>
                {selectedUnitIds.length === movableUnits.length ? 'Deselect' : 'Select All'}
              </button>
            )}
          </div>

          {/* Move Selected button */}
          {hasSelection && isPlayerProvince && (
            <button
              onClick={onMoveSelected}
              style={{
                ...btnStyle('#a85'),
                width: '100%', marginBottom: 4, padding: '3px 6px',
              }}
            >
              Move {selectedUnitIds.length} Unit{selectedUnitIds.length > 1 ? 's' : ''}
            </button>
          )}

          {army_units.map(unit => {
            const pending = pendingMoves.find(m => m.unit_id === unit.id);
            const icon = CATEGORY_ICONS[unit.category] || '';
            const stars = '\u2605'.repeat(unit.medals);
            const isMovable = unit.category !== 'Garrison' && !pending;
            const isSelected = selectedUnitIds.includes(unit.id);
            return (
              <div key={unit.id} style={{
                background: isSelected ? 'rgba(218,165,32,0.15)' : 'rgba(255,255,255,0.05)',
                borderRadius: 4,
                padding: '4px 6px',
                marginBottom: 3,
                border: isSelected ? '1px solid rgba(218,165,32,0.4)' : '1px solid transparent',
                cursor: isMovable && isPlayerProvince ? 'pointer' : 'default',
              }}
                onClick={(e) => {
                  if (isMovable && isPlayerProvince) onToggleUnit(unit.id, e.shiftKey);
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <span>
                    {isMovable && isPlayerProvince && (
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
                {pending ? (
                  <div style={{ marginTop: 3, fontSize: 11 }}>
                    <span style={{ color: '#ffd700' }}>
                      \u2192 {pending.destination_name}
                    </span>
                    <button onClick={(e) => { e.stopPropagation(); onCancelMove(unit.id); }} style={btnStyle('#a33')}>
                      Cancel
                    </button>
                  </div>
                ) : (
                  !hasSelection && isPlayerProvince && unit.category !== 'Garrison' && (
                    <button onClick={(e) => { e.stopPropagation(); onMoveUnit(unit.id); }} style={{ ...btnStyle('#456'), marginTop: 3 }}>
                      Move
                    </button>
                  )
                )}
              </div>
            );
          })}
        </>
      )}

      {army_units.length === 0 && garrison_count === 0 && (
        <div style={{ color: '#888', fontStyle: 'italic' }}>No units in province</div>
      )}

      {/* Recruitment section — only at player's country capital */}
      {isPlayerCapital && (
        <div style={{ marginTop: 10, borderTop: '1px solid #3a3520', paddingTop: 8 }}>
          <div style={{ fontWeight: 'bold', marginBottom: 4, color: '#ccc' }}>Recruit Army Unit</div>
          <div style={{ fontSize: 11, color: '#888', marginBottom: 6 }}>
            Treasury: ${treasury.toLocaleString()} | Arms: {arms}
          </div>
          {buildableArmy.map(b => {
            const canBuild = b.can_afford && b.tech_met;
            return (
              <div key={b.type} style={{
                display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                padding: '2px 0',
                opacity: canBuild ? 1 : 0.45,
              }}>
                <span style={{ fontSize: 12 }}>
                  {b.type.replace(/([A-Z])/g, ' $1').trim()}
                  <span style={{ color: '#888', fontSize: 10, marginLeft: 4 }}>
                    ${b.cost} + {b.arms_required}A
                  </span>
                </span>
                {canBuild ? (
                  <button onClick={() => onRecruit(b.type)} style={btnStyle('#2a6')}>
                    Recruit
                  </button>
                ) : (
                  <span style={{ fontSize: 10, color: '#a66' }}>
                    {b.reason || 'Unavailable'}
                  </span>
                )}
              </div>
            );
          })}
        </div>
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
