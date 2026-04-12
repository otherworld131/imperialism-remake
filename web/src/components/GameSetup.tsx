import { useState, useEffect } from 'react';
import { getScenarios, newGame, newScenarioGame } from '../wasm';

const NATION_COLORS: Record<string, string> = {
  Yellow: '#ffd900', Orange: '#ff8c00', LightBlue: '#66b3ff',
  Red: '#e62626', Green: '#1abf1a', Purple: '#a633d9',
  Blue: '#3359e6', Gray: '#999', Brown: '#8c5926',
  Pink: '#ff80b3', Teal: '#00b3a6', Olive: '#808000',
};

const DIFFICULTIES = ['Introductory', 'Easy', 'Normal', 'Hard', 'NOI'];

interface Props {
  onStartGame: (gameJson: string) => void;
}

export default function GameSetup({ onStartGame }: Props) {
  const [scenarios, setScenarios] = useState<any[]>([]);
  const [selectedScenario, setSelectedScenario] = useState<string | null>(null);
  const [difficulty, setDifficulty] = useState(2);
  const [nationIndex, setNationIndex] = useState(0);
  const [mapKey, setMapKey] = useState('');

  useEffect(() => {
    try { setScenarios(getScenarios()); } catch { /* no scenarios available */ }
  }, []);

  const handleStart = () => {
    let json: string;
    if (selectedScenario) {
      json = newScenarioGame(selectedScenario, difficulty, nationIndex);
    } else {
      json = newGame(mapKey || 'imperialism', difficulty, nationIndex);
    }
    onStartGame(json);
  };

  return (
    <div style={s.page}>
      <div style={s.container}>
        <div style={s.header}>
          <h1 style={s.headerTitle}>Imperialism</h1>
          <p style={s.headerSub}>A game of diplomacy, trade, and conquest in the age of empire</p>
        </div>

        <div style={s.body}>
          {/* Scenario */}
          <div style={s.group}>
            <label style={s.label}>Scenario</label>
            <div style={s.cards}>
              <div
                style={selectedScenario === null ? { ...s.card, ...s.cardSelected } : s.card}
                onClick={() => setSelectedScenario(null)}
              >
                <div style={s.cardIcon}>&#127758;</div>
                <div style={s.cardName}>Random Map</div>
                <div style={s.cardDesc}>Procedurally generated world</div>
              </div>
              {scenarios.map((sc: any) => (
                <div
                  key={sc.id}
                  style={selectedScenario === sc.id ? { ...s.card, ...s.cardSelected } : s.card}
                  onClick={() => setSelectedScenario(sc.id)}
                >
                  <div style={s.cardIcon}>&#128214;</div>
                  <div style={s.cardName}>{sc.name || sc.id}</div>
                  <div style={s.cardDesc}>{sc.description || `Year ${sc.year || '?'}`}</div>
                </div>
              ))}
            </div>
          </div>

          {/* Difficulty */}
          <div style={s.group}>
            <label style={s.label}>Difficulty</label>
            <div style={s.diffRow}>
              {DIFFICULTIES.map((d, i) => (
                <div
                  key={d}
                  style={difficulty === i ? { ...s.diffBtn, ...s.diffSelected } : s.diffBtn}
                  onClick={() => setDifficulty(i)}
                >
                  {d}
                </div>
              ))}
            </div>
          </div>

          {/* Nation picker */}
          <div style={s.group}>
            <label style={s.label}>Choose Your Empire</label>
            <div style={s.nationGrid}>
              {Object.entries(NATION_COLORS).slice(0, 7).map(([name, color], i) => (
                <div
                  key={name}
                  style={nationIndex === i ? { ...s.nationPick, ...s.nationSelected } : s.nationPick}
                  onClick={() => setNationIndex(i)}
                >
                  <div style={{ ...s.swatch, background: color }} />
                  <div style={s.nationName}>Nation {i + 1}</div>
                </div>
              ))}
            </div>
          </div>

          {/* Map key */}
          {!selectedScenario && (
            <div style={s.group}>
              <label style={s.label}>Map Key (optional)</label>
              <div style={s.mapKeyRow}>
                <input
                  style={s.mapKeyInput}
                  placeholder="Leave blank for random..."
                  maxLength={32}
                  value={mapKey}
                  onChange={e => setMapKey(e.target.value)}
                />
              </div>
            </div>
          )}
        </div>

        <div style={s.footer}>
          <button style={s.startBtn} onClick={handleStart}>Begin Campaign</button>
        </div>
      </div>
    </div>
  );
}

const s: Record<string, React.CSSProperties> = {
  page: { fontFamily: 'Georgia, serif', background: '#1a1a2e', color: '#e0d8c0', height: '100vh', display: 'flex', justifyContent: 'center', alignItems: 'center' },
  container: { width: 700, background: '#161625', border: '2px solid #3a3520', borderRadius: 4, overflow: 'hidden' },
  header: { background: '#0f0f23', padding: '20px 30px', borderBottom: '2px solid #3a3520', textAlign: 'center' as const },
  headerTitle: { fontSize: 28, color: '#daa520', margin: 0, fontWeight: 'normal' },
  headerSub: { fontSize: 13, color: '#9a9a9a', margin: '4px 0 0' },
  body: { padding: '24px 30px' },
  group: { marginBottom: 20 },
  label: { display: 'block', fontSize: 13, color: '#daa520', marginBottom: 6, textTransform: 'uppercase' as const, letterSpacing: 0.5 },
  cards: { display: 'flex', gap: 12 },
  card: { flex: 1, padding: 14, background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 4, cursor: 'pointer', textAlign: 'center' as const },
  cardSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)' },
  cardIcon: { fontSize: 28, marginBottom: 6 },
  cardName: { fontSize: 14, fontWeight: 'bold' as const },
  cardDesc: { fontSize: 11, color: '#9a9a9a', marginTop: 4 },
  diffRow: { display: 'flex', gap: 8 },
  diffBtn: { flex: 1, padding: 8, background: '#1a1a2e', border: '1px solid #3a3520', color: '#e0d8c0', fontFamily: 'Georgia, serif', fontSize: 12, cursor: 'pointer', borderRadius: 3, textAlign: 'center' as const },
  diffSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)', color: '#daa520' },
  nationGrid: { display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 8 },
  nationPick: { padding: '10px 8px', background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 4, cursor: 'pointer', textAlign: 'center' as const },
  nationSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)' },
  swatch: { width: 20, height: 20, borderRadius: '50%', margin: '0 auto 6px', border: '1px solid rgba(255,255,255,0.2)' },
  nationName: { fontSize: 12 },
  mapKeyRow: { display: 'flex', gap: 10, alignItems: 'center' },
  mapKeyInput: { flex: 1, padding: '6px 10px', background: '#1a1a2e', border: '1px solid #3a3520', color: '#e0d8c0', fontFamily: "'Courier New', monospace", fontSize: 13, borderRadius: 3 },
  footer: { padding: '16px 30px', background: '#0f0f23', borderTop: '2px solid #3a3520', display: 'flex', justifyContent: 'flex-end' },
  startBtn: { padding: '10px 40px', background: '#8b4513', color: '#fff', border: '1px solid #a0522d', fontFamily: 'Georgia, serif', fontSize: 16, fontWeight: 'bold' as const, cursor: 'pointer', borderRadius: 3, letterSpacing: 0.5 },
};
