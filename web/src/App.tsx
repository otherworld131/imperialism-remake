import { useState, useEffect } from 'react';
import { initWasm, newGame, processTurn, getMapData, getAvailableTechs, researchTech } from './wasm';
import type { TileData } from './wasm';
import HexMap from './components/HexMap';

function App() {
  const [loading, setLoading] = useState(true);
  const [gameJson, setGameJson] = useState<string>('');
  const [tiles, setTiles] = useState<TileData[]>([]);
  const [gameState, setGameState] = useState<any>(null);
  const [selectedTile, setSelectedTile] = useState<TileData | null>(null);
  const [hoveredTile, setHoveredTile] = useState<TileData | null>(null);
  const [headlines, setHeadlines] = useState<string[]>([]);
  const [showNewspaper, setShowNewspaper] = useState(false);
  const [techs, setTechs] = useState<any[]>([]);
  const [showTech, setShowTech] = useState(false);

  useEffect(() => {
    (async () => {
      await initWasm();
      const json = newGame('imperialism', 2, 0);
      setGameJson(json);
      const state = JSON.parse(json);
      setGameState(state);
      setTiles(getMapData(json));
      setTechs(getAvailableTechs(json));
      setLoading(false);
    })();
  }, []);

  const handleEndTurn = () => {
    const result = processTurn(gameJson);
    if (result.error) { alert(result.error); return; }
    const newJson = JSON.stringify(result.game);
    setGameJson(newJson);
    setGameState(result.game);
    setTiles(getMapData(newJson));
    setTechs(getAvailableTechs(newJson));
    setHeadlines(result.report?.headlines || []);
    setShowNewspaper(true);
  };

  const handleResearch = (techName: string) => {
    const result = researchTech(gameJson, techName);
    const parsed = JSON.parse(result);
    if (parsed.error) { alert(parsed.error); return; }
    setGameJson(result);
    setGameState(parsed);
    setTechs(getAvailableTechs(result));
    setShowTech(false);
  };

  if (loading) return <div style={styles.loading}>Loading Imperialism...</div>;

  const player = gameState?.nations?.find((n: any) => n.id === gameState.human_player_nation);
  const turnNumber = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
  const year = 1815 + Math.floor((turnNumber - 1) / 4);
  const quarter = ((turnNumber - 1) % 4) + 1;

  return (
    <div style={styles.container}>
      {/* Top bar */}
      <div style={styles.topBar}>
        <span style={styles.title}>Empire of {player?.name || '?'}</span>
        <span>Turn {turnNumber} ({year} Q{quarter})</span>
        <span>Treasury: ${player?.treasury?.[0] ? player.treasury[0] / 100 : 0}</span>
        <span>Provinces: {player?.province_ids?.length || 0}</span>
        <button onClick={() => setShowTech(!showTech)} style={styles.btn}>Tech</button>
        <button onClick={handleEndTurn} style={styles.endTurnBtn}>End Turn</button>
      </div>

      {/* Main area */}
      <div style={styles.mainArea}>
        <div style={styles.mapContainer}>
          <HexMap tiles={tiles} onTileClick={setSelectedTile} onTileHover={setHoveredTile} />
        </div>

        {/* Side panel */}
        <div style={styles.sidePanel}>
          <h3 style={styles.panelTitle}>Tile Info</h3>
          {(hoveredTile || selectedTile) ? (
            <div style={styles.tileInfo}>
              <p><b>{(hoveredTile || selectedTile)!.terrain}</b></p>
              <p>Province: {(hoveredTile || selectedTile)!.province || 'None'}</p>
              <p>Owner: {(hoveredTile || selectedTile)!.owner || 'None'}</p>
              <p>Level: {(hoveredTile || selectedTile)!.improvement_level}</p>
              {(hoveredTile || selectedTile)!.is_capital && <p>{'\u2605'} Capital</p>}
              {(hoveredTile || selectedTile)!.has_railroad && <p>Railroad</p>}
              {(hoveredTile || selectedTile)!.has_fort && <p>Fort L{(hoveredTile || selectedTile)!.fort_level}</p>}
            </div>
          ) : <p style={styles.hint}>Hover over a tile</p>}

          <h3 style={styles.panelTitle}>Nations</h3>
          <div style={styles.nationList}>
            {gameState?.nations?.filter((n: any) => n.nation_type === 'GreatPower').map((n: any) => (
              <div key={n.id} style={styles.nationItem}>
                <span>{n.name}</span>
                <span>{n.province_ids?.length || 0} prov</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Newspaper modal */}
      {showNewspaper && (
        <div style={styles.modal} onClick={() => setShowNewspaper(false)}>
          <div style={styles.modalContent} onClick={e => e.stopPropagation()}>
            <h2 style={styles.newspaperTitle}>The Imperial Times</h2>
            {headlines.map((h, i) => <p key={i} style={styles.headline}>{h}</p>)}
            <button onClick={() => setShowNewspaper(false)} style={styles.btn}>Continue</button>
          </div>
        </div>
      )}

      {/* Tech panel */}
      {showTech && (
        <div style={styles.modal} onClick={() => setShowTech(false)}>
          <div style={styles.modalContent} onClick={e => e.stopPropagation()}>
            <h2>Available Technologies</h2>
            {techs.length === 0 ? <p>No technologies available this year.</p> :
              techs.map((t, i) => (
                <div key={i} style={styles.techItem}>
                  <span>{t.name} (${t.cost})</span>
                  <button onClick={() => handleResearch(t.name)} style={styles.btn}>Research</button>
                </div>
              ))
            }
            <button onClick={() => setShowTech(false)} style={styles.btn}>Close</button>
          </div>
        </div>
      )}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: { display: 'flex', flexDirection: 'column', height: '100vh', fontFamily: "'Georgia', serif", background: '#1a1a2e', color: '#e0d8c0' },
  loading: { display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh', fontSize: 24, color: '#c0a060' },
  topBar: { display: 'flex', alignItems: 'center', gap: 20, padding: '8px 16px', background: '#0f0f23', borderBottom: '2px solid #3a3520' },
  title: { fontWeight: 'bold', fontSize: 18, color: '#daa520' },
  mainArea: { display: 'flex', flex: 1, overflow: 'hidden' },
  mapContainer: { flex: 1, background: '#0a0a1a' },
  sidePanel: { width: 260, padding: 12, background: '#161625', borderLeft: '2px solid #3a3520', overflowY: 'auto' },
  panelTitle: { margin: '12px 0 6px', color: '#daa520', borderBottom: '1px solid #3a3520', paddingBottom: 4 },
  tileInfo: { fontSize: 13 },
  hint: { color: '#666', fontStyle: 'italic' },
  nationList: { fontSize: 13 },
  nationItem: { display: 'flex', justifyContent: 'space-between', padding: '2px 0' },
  btn: { padding: '4px 12px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', cursor: 'pointer', fontFamily: 'Georgia, serif' },
  endTurnBtn: { padding: '6px 20px', background: '#8b4513', color: '#fff', border: '1px solid #a0522d', cursor: 'pointer', fontWeight: 'bold', fontFamily: 'Georgia, serif' },
  modal: { position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', justifyContent: 'center', alignItems: 'center', zIndex: 100 },
  modalContent: { background: '#1a1a2e', border: '2px solid #daa520', padding: 24, maxWidth: 500, maxHeight: '80vh', overflowY: 'auto' },
  newspaperTitle: { fontFamily: "'Times New Roman', serif", textAlign: 'center', color: '#daa520', borderBottom: '2px double #daa520', paddingBottom: 8 },
  headline: { margin: '6px 0', fontSize: 14 },
  techItem: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '4px 0' },
};

export default App;
