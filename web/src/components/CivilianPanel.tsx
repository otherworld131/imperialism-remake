import type { CiviliansData, CivilianDetail } from '../wasm';
import { resourceLabel } from '../resourceEmoji';

const CIVILIAN_EMOJI: Record<string, string> = {
  Farmer: '\u{1F33E}',
  Miner: '⛏️',
  Engineer: '\u{1F527}',
  Forester: '\u{1FAA3}',
  Rancher: '\u{1F920}',
  Driller: '\u{1F6E2}️',
  Prospector: '\u{1F50D}',
};

interface Props {
  civilians: CiviliansData;
  selectedCivilianId?: number | null;
  onSelectCivilian: (civilian: CivilianDetail) => void;
}

export default function CivilianPanel({
  civilians,
  selectedCivilianId,
  onSelectCivilian,
}: Props) {
  const { deployed, undeployed } = civilians;

  return (
    <div style={{ fontSize: 'var(--ui-font-size, 14px)' }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6, color: '#ccc' }}>Civilian Workforce</div>

      {/* Undeployed */}
      {undeployed.length > 0 && (
        <div style={{ marginBottom: 8 }}>
          <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#888', marginBottom: 3 }}>
            Undeployed ({undeployed.length})
          </div>
          {undeployed.map(civ => (
            <div
              key={civ.id}
              role="button"
              tabIndex={0}
              onClick={() => onSelectCivilian(civ)}
              onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelectCivilian(civ); } }}
              style={{
                display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                padding: '4px 6px', background: 'rgba(46,204,64,0.08)', borderRadius: 3, marginBottom: 2,
                cursor: 'pointer', border: '1px solid transparent',
              }}
              onMouseEnter={e => (e.currentTarget.style.background = 'rgba(46,204,64,0.18)')}
              onMouseLeave={e => (e.currentTarget.style.background = 'rgba(46,204,64,0.08)')}
            >
              <span>{CIVILIAN_EMOJI[civ.type] || ''} {civ.type}</span>
              <span style={{ fontSize: 10, color: '#8c8' }}>Click to deploy</span>
            </div>
          ))}
        </div>
      )}

      {/* Deployed */}
      {deployed.length > 0 && (
        <div style={{ marginBottom: 8 }}>
          <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#888', marginBottom: 3 }}>
            Deployed ({deployed.length})
          </div>
          {deployed.map(civ => {
            const isSelected = selectedCivilianId === civ.id;
            return (
              <div
                key={civ.id}
                role="button"
                tabIndex={0}
                onClick={() => onSelectCivilian(civ)}
                onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelectCivilian(civ); } }}
                style={{
                  background: isSelected ? 'rgba(255,255,255,0.12)' : 'rgba(255,255,255,0.05)',
                  borderRadius: 4, padding: '4px 6px', marginBottom: 3,
                  cursor: 'pointer',
                  border: isSelected ? '1px solid rgba(255,255,255,0.25)' : '1px solid transparent',
                }}
                onMouseEnter={e => { if (!isSelected) e.currentTarget.style.background = 'rgba(255,255,255,0.09)'; }}
                onMouseLeave={e => { if (!isSelected) e.currentTarget.style.background = 'rgba(255,255,255,0.05)'; }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <span>
                    {CIVILIAN_EMOJI[civ.type] || ''} {civ.type}
                    {civ.position && (
                      <span style={{ fontSize: 10, color: '#888', marginLeft: 4 }}>
                        ({civ.position.q},{civ.position.r})
                      </span>
                    )}
                  </span>
                  {!civ.working
                    ? <span style={{ fontSize: 10, color: '#8c8' }}>Click to redeploy</span>
                    : isSelected && <span style={{ fontSize: 10, color: '#aaa' }}>selected ▶ map</span>
                  }
                </div>
                {civ.working && civ.turns_remaining > 0 && (
                  <div style={{ marginTop: 2 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <ProgressBar turns={civ.turns_remaining} maxTurns={5} />
                      <span style={{ fontSize: 10, color: '#888' }}>
                        {civ.turns_remaining} turn{civ.turns_remaining !== 1 ? 's' : ''}
                      </span>
                    </div>
                    {civ.tile_resource && (
                      <div style={{ fontSize: 10, color: '#999' }}>
                        Improving {resourceLabel(civ.tile_resource)}
                      </div>
                    )}
                    {civ.type === 'Engineer' && civ.build_task && (
                      <div style={{ fontSize: 10, color: '#8c8', fontStyle: 'italic' }}>
                        Building {civ.build_task}
                      </div>
                    )}
                  </div>
                )}
                {!civ.working && (
                  <div style={{ fontSize: 10, color: '#999', fontStyle: 'italic' }}>Idle</div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {deployed.length === 0 && undeployed.length === 0 && (
        <div style={{ color: '#888', fontStyle: 'italic', marginBottom: 8 }}>No civilians</div>
      )}

    </div>
  );
}

function ProgressBar({ turns, maxTurns }: { turns: number; maxTurns: number }) {
  const filled = maxTurns - turns;
  const pct = Math.min(100, (filled / maxTurns) * 100);
  return (
    <div style={{ width: 50, height: 5, background: 'rgba(255,255,255,0.1)', borderRadius: 2, overflow: 'hidden' }}>
      <div style={{ width: `${pct}%`, height: '100%', background: '#4a8' }} />
    </div>
  );
}
