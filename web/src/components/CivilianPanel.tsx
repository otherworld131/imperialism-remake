import type { CiviliansData, CivilianDetail, BuildableUnit } from '../wasm';

const CIVILIAN_EMOJI: Record<string, string> = {
  Farmer: '\u{1F33E}',
  Miner: '\u26CF\uFE0F',
  Engineer: '\u{1F527}',
  Forester: '\u{1FAA3}',
  Rancher: '\u{1F920}',
  Driller: '\u{1F6E2}\uFE0F',
  Prospector: '\u{1F50D}',
};

interface Props {
  civilians: CiviliansData;
  buildableCivilians: BuildableUnit[];
  treasury: number;
  onDeploy: (civilian: CivilianDetail) => void;
  onRecall: (civilianId: number) => void;
  onHire: (civilianType: string) => void;
}

export default function CivilianPanel({
  civilians, buildableCivilians, treasury,
  onDeploy, onRecall, onHire,
}: Props) {
  const { deployed, undeployed } = civilians;

  return (
    <div style={{ fontSize: 13 }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6, color: '#ccc' }}>Civilian Workforce</div>

      {/* Undeployed */}
      {undeployed.length > 0 && (
        <div style={{ marginBottom: 8 }}>
          <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>
            Undeployed ({undeployed.length})
          </div>
          {undeployed.map(civ => (
            <div key={civ.id} style={{
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              padding: '2px 4px', background: 'rgba(255,255,255,0.05)', borderRadius: 3, marginBottom: 2,
            }}>
              <span>{CIVILIAN_EMOJI[civ.type] || ''} {civ.type}</span>
              <button
                onClick={() => onDeploy(civ)}
                style={{ background: '#2a6', color: '#fff', border: 'none', borderRadius: 3, padding: '1px 6px', fontSize: 10, cursor: 'pointer' }}
              >
                Deploy
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Deployed */}
      {deployed.length > 0 && (
        <div style={{ marginBottom: 8 }}>
          <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>
            Deployed ({deployed.length})
          </div>
          {deployed.map(civ => (
            <div key={civ.id} style={{
              background: 'rgba(255,255,255,0.05)', borderRadius: 4, padding: '4px 6px', marginBottom: 3,
            }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span>
                  {CIVILIAN_EMOJI[civ.type] || ''} {civ.type}
                  {civ.position && (
                    <span style={{ fontSize: 10, color: '#888', marginLeft: 4 }}>
                      ({civ.position.q},{civ.position.r})
                    </span>
                  )}
                </span>
                <button
                  onClick={() => onRecall(civ.id)}
                  style={{ background: '#a63', color: '#fff', border: 'none', borderRadius: 3, padding: '1px 6px', fontSize: 10, cursor: 'pointer' }}
                >
                  Recall
                </button>
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
                      Improving {civ.tile_resource}
                    </div>
                  )}
                </div>
              )}
              {!civ.working && (
                <div style={{ fontSize: 10, color: '#999', fontStyle: 'italic' }}>Idle</div>
              )}
            </div>
          ))}
        </div>
      )}

      {deployed.length === 0 && undeployed.length === 0 && (
        <div style={{ color: '#888', fontStyle: 'italic', marginBottom: 8 }}>No civilians</div>
      )}

      {/* Hire section */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4, color: '#ccc' }}>Hire Civilian</div>
        <div style={{ fontSize: 11, color: '#888', marginBottom: 4 }}>
          Treasury: ${treasury.toLocaleString()}
        </div>
        {buildableCivilians.map(b => {
          const canBuild = b.can_afford && b.tech_met;
          return (
            <div key={b.type} style={{
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              padding: '2px 0', opacity: canBuild ? 1 : 0.45,
            }}>
              <span style={{ fontSize: 12 }}>
                {CIVILIAN_EMOJI[b.type] || ''} {b.type}
                <span style={{ color: '#888', fontSize: 10, marginLeft: 4 }}>${b.cost}</span>
              </span>
              {canBuild ? (
                <button
                  onClick={() => onHire(b.type)}
                  style={{ background: '#2a6', color: '#fff', border: 'none', borderRadius: 3, padding: '1px 6px', fontSize: 10, cursor: 'pointer' }}
                >
                  Hire
                </button>
              ) : (
                <span style={{ fontSize: 10, color: '#a66' }}>{b.reason || 'Unavailable'}</span>
              )}
            </div>
          );
        })}
      </div>
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
