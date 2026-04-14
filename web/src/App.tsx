import { useState, useEffect, useCallback } from 'react';
import { initWasm, processTurn, getMapData, getAvailableTechs, researchTech } from './wasm';
import type { TileData, Headline } from './wasm';

const CATEGORY_COLORS: Record<string, string> = {
  war:       '#e63946',
  battle:    '#e76f51',
  diplomacy: '#457b9d',
  growth:    '#2a9d8f',
  trade:     '#daa520',
  crisis:    '#9d0208',
  politics:  '#b380e6',
  military:  '#8a9aaf',
  default:   '#e0d8c0',
};

type ScreenTab = 'map' | 'transport' | 'industry' | 'trade' | 'diplomacy';
const SCREEN_TABS: { key: ScreenTab; label: string; hotkey: string }[] = [
  { key: 'map', label: 'Map', hotkey: 'F1' },
  { key: 'transport', label: 'Transport', hotkey: 'F2' },
  { key: 'industry', label: 'Industry', hotkey: 'F3' },
  { key: 'trade', label: 'Trade', hotkey: 'F4' },
  { key: 'diplomacy', label: 'Diplomacy', hotkey: 'F5' },
];

function extractNationTag(text: string, nations?: any[]): string | null {
  if (!nations) return null;
  for (const n of nations) {
    if (n.nation_type === 'GreatPower' && text.includes(n.name)) return n.name;
  }
  return null;
}

import HexMap from './components/HexMap';
import GameSetup from './components/GameSetup';

function App() {
  const [loading, setLoading] = useState(true);
  const [gameJson, setGameJson] = useState<string>('');
  const [tiles, setTiles] = useState<TileData[]>([]);
  const [gameState, setGameState] = useState<any>(null);
  const [selectedTile, setSelectedTile] = useState<TileData | null>(null);
  const [hoveredTile, setHoveredTile] = useState<TileData | null>(null);
  const [headlines, setHeadlines] = useState<Headline[]>([]);
  const [showNewspaper, setShowNewspaper] = useState(false);
  const [techs, setTechs] = useState<any[]>([]);
  const [showTech, setShowTech] = useState(false);
  const [activeScreen, setActiveScreen] = useState<ScreenTab>('map');
  const [gameStarted, setGameStarted] = useState(false);
  const [showHiddenResources, setShowHiddenResources] = useState(false);

  useEffect(() => {
    (async () => {
      await initWasm();
      setLoading(false);
    })();
  }, []);

  const handleGameStart = (json: string) => {
    setGameJson(json);
    const state = JSON.parse(json);
    setGameState(state);
    setTiles(getMapData(json));
    setTechs(getAvailableTechs(json));
    setGameStarted(true);
  };

  const handleEndTurn = useCallback(() => {
    const result = processTurn(gameJson);
    if (result.error) { alert(result.error); return; }
    const newJson = JSON.stringify(result.game);
    setGameJson(newJson);
    setGameState(result.game);
    setTiles(getMapData(newJson));
    setTechs(getAvailableTechs(newJson));
    setHeadlines(result.report?.headlines || []);
    setShowNewspaper(true);
  }, [gameJson]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !showNewspaper && !showTech) {
        e.preventDefault();
        handleEndTurn();
      }
      if (e.code === 'Escape') {
        if (showNewspaper) setShowNewspaper(false);
        else if (showTech) setShowTech(false);
      }
      if (e.code === 'F1') { e.preventDefault(); setActiveScreen('map'); }
      if (e.code === 'F2') { e.preventDefault(); setActiveScreen('transport'); }
      if (e.code === 'F3') { e.preventDefault(); setActiveScreen('industry'); }
      if (e.code === 'F4') { e.preventDefault(); setActiveScreen('trade'); }
      if (e.code === 'F5') { e.preventDefault(); setActiveScreen('diplomacy'); }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showNewspaper, showTech, handleEndTurn]);

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
  if (!gameStarted) return <GameSetup onStartGame={handleGameStart} />;

  const player = gameState?.nations?.find((n: any) => n.id === gameState.human_player_nation);
  const turnNumber = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
  const year = 1815 + Math.floor((turnNumber - 1) / 4);
  const quarter = ((turnNumber - 1) % 4) + 1;
  const playerName = player?.name || '?';

  return (
    <main style={styles.container}>
      {/* Top bar */}
      <div style={styles.topBar} className="top-bar-responsive">
        <span style={styles.title} className="title-text">Empire of {playerName}</span>
        <span>Turn {turnNumber} ({year} Q{quarter})</span>
        <span>Treasury: ${player?.treasury?.[0] ? player.treasury[0] / 100 : 0}</span>
        <span>Provinces: {player?.province_ids?.length || 0}</span>
        <button onClick={() => setShowTech(!showTech)} style={styles.btn}>Tech</button>
        <button onClick={handleEndTurn} style={styles.endTurnBtn}>End Turn</button>
      </div>

      {/* Screen tabs */}
      <div style={styles.screenTabs} className="screen-tabs-responsive">
        {SCREEN_TABS.map(tab => (
          <button
            key={tab.key}
            style={activeScreen === tab.key ? { ...styles.screenTab, ...styles.screenTabActive } : styles.screenTab}
            onClick={() => setActiveScreen(tab.key)}
          >
            {tab.label}
            <span style={activeScreen === tab.key ? styles.hotkeyActive : styles.hotkey}>{tab.hotkey}</span>
          </button>
        ))}
      </div>

      {/* Main area */}
      <div style={styles.mainArea} className="main-area-responsive">
        <div style={styles.mapContainer}>
          <HexMap tiles={tiles} onTileClick={setSelectedTile} onTileHover={setHoveredTile} showHiddenResources={showHiddenResources} />
        </div>

        {/* Side panel — context-sensitive */}
        <div style={styles.sidePanel} className="side-panel-responsive">
          {activeScreen === 'map' && (
            <>
              <h3 style={styles.panelTitle}>Tile Info</h3>
              {selectedTile && (
                <div style={styles.tileSelected}>
                  <div style={styles.tileLabel}>Selected</div>
                  <div style={styles.tileInfo}>
                    <p><b>{selectedTile.terrain}{selectedTile.resource && (!selectedTile.resource_hidden || showHiddenResources) ? ` — ${selectedTile.resource}` : ''}</b></p>
                    <p>Province: {selectedTile.province || 'None'}</p>
                    <p>Owner: {selectedTile.owner || 'None'}</p>
                    {selectedTile.resource && (!selectedTile.resource_hidden || showHiddenResources) && <p>Level: {selectedTile.improvement_level}</p>}
                    {selectedTile.is_capital && <p>{'\u2605'} Capital</p>}
                    {selectedTile.has_railroad && <p>Railroad</p>}
                    {selectedTile.has_fort && <p>Fort L{selectedTile.fort_level}</p>}
                  </div>
                </div>
              )}
              {hoveredTile && !(selectedTile && hoveredTile.q === selectedTile.q && hoveredTile.r === selectedTile.r) && (
                <div style={styles.tileHovered}>
                  <div style={styles.tileLabelDim}>Hovering</div>
                  <div style={styles.tileInfo}>
                    <p><b>{hoveredTile.terrain}{hoveredTile.resource && (!hoveredTile.resource_hidden || showHiddenResources) ? ` — ${hoveredTile.resource}` : ''}</b></p>
                    <p>Province: {hoveredTile.province || 'None'}</p>
                    <p>Owner: {hoveredTile.owner || 'None'}</p>
                    {hoveredTile.resource && (!hoveredTile.resource_hidden || showHiddenResources) && <p>Level: {hoveredTile.improvement_level}</p>}
                    {hoveredTile.is_capital && <p>{'\u2605'} Capital</p>}
                    {hoveredTile.has_railroad && <p>Railroad</p>}
                    {hoveredTile.has_fort && <p>Fort L{hoveredTile.fort_level}</p>}
                  </div>
                </div>
              )}
              {!selectedTile && !hoveredTile && (
                <p style={styles.hint}>Click to pin, hover to preview</p>
              )}

              <div style={{ padding: '4px 0', fontSize: '12px' }}>
                <label>
                  <input type="checkbox" checked={showHiddenResources} onChange={e => setShowHiddenResources(e.target.checked)} />
                  {' '}Show hidden resources (debug)
                </label>
              </div>

              <h3 style={styles.panelTitle}>Nations</h3>
              <div style={styles.nationList}>
                {gameState?.nations?.filter((n: any) => n.nation_type === 'GreatPower').map((n: any) => (
                  <div key={n.id} style={styles.nationItem}>
                    <span>{n.name}</span>
                    <span>{n.province_ids?.length || 0} prov</span>
                  </div>
                ))}
              </div>
            </>
          )}
          {activeScreen === 'transport' && (
            <>
              <h3 style={styles.panelTitle}>Transport Network</h3>
              <p style={styles.hint}>Rail routes and depots will appear here.</p>
              <h3 style={styles.panelTitle}>Freight Cars</h3>
              <p style={styles.hint}>Resource allocation will appear here.</p>
            </>
          )}
          {activeScreen === 'industry' && (
            <>
              <h3 style={styles.panelTitle}>Buildings</h3>
              <p style={styles.hint}>Capital city buildings will appear here.</p>
              <h3 style={styles.panelTitle}>Workforce</h3>
              <p style={styles.hint}>Worker assignments will appear here.</p>
              <h3 style={styles.panelTitle}>Warehouse</h3>
              <p style={styles.hint}>Resource stockpiles will appear here.</p>
            </>
          )}
          {activeScreen === 'trade' && (
            <>
              <h3 style={styles.panelTitle}>Trade Partners</h3>
              <p style={styles.hint}>Available trade deals will appear here.</p>
              <h3 style={styles.panelTitle}>Market Prices</h3>
              <p style={styles.hint}>Commodity prices will appear here.</p>
            </>
          )}
          {activeScreen === 'diplomacy' && (
            <>
              <h3 style={styles.panelTitle}>Relations</h3>
              <p style={styles.hint}>Diplomatic relations will appear here.</p>
              <h3 style={styles.panelTitle}>Treaties</h3>
              <p style={styles.hint}>Active treaties will appear here.</p>
            </>
          )}
        </div>
      </div>

      {/* Newspaper modal — grouped */}
      {showNewspaper && (() => {
        const playerNews = headlines.filter(h => h.text.includes(playerName));
        const worldNews = headlines.filter(h => !h.text.includes(playerName));
        return (
          <div style={styles.modal} onClick={() => setShowNewspaper(false)}>
            <div style={styles.newspaperModal} onClick={e => e.stopPropagation()}>
              <div style={styles.masthead}>
                <h2 style={styles.newspaperTitle}>The Imperial Times</h2>
                <div style={styles.mastheadDate}>{year} Q{quarter} — Turn {turnNumber}</div>
              </div>
              <div style={styles.newsBody}>
                {playerNews.length > 0 && (
                  <div style={{ marginBottom: 16 }}>
                    <div style={styles.sectionLabelPlayer}>Your Empire — {playerName}</div>
                    {playerNews.map((h, i) => (
                      <div key={i} style={{ ...styles.headlineRow, borderLeftColor: CATEGORY_COLORS[h.category] || '#3a3520', color: CATEGORY_COLORS[h.category] || '#e0d8c0' }}>
                        {h.text}
                      </div>
                    ))}
                  </div>
                )}
                {worldNews.length > 0 && (
                  <div>
                    <div style={styles.sectionLabelWorld}>World News</div>
                    {worldNews.map((h, i) => {
                      const tag = extractNationTag(h.text, gameState?.nations);
                      return (
                        <div key={i} style={{ ...styles.headlineRow, borderLeftColor: CATEGORY_COLORS[h.category] || '#3a3520', color: CATEGORY_COLORS[h.category] || '#e0d8c0' }}>
                          {tag && <span style={styles.nationTag}>{tag}</span>}
                          {h.text}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
              <div style={styles.newsFooter}>
                <span style={{ fontSize: 11, color: '#666' }}>Esc to dismiss</span>
                <button onClick={() => setShowNewspaper(false)} style={styles.endTurnBtn}>Continue</button>
              </div>
            </div>
          </div>
        );
      })()}

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
    </main>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: { display: 'flex', flexDirection: 'column', height: '100vh', fontFamily: "'Georgia', serif", background: '#1a1a2e', color: '#e0d8c0' },
  loading: { display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh', fontSize: 24, color: '#c0a060' },
  topBar: { display: 'flex', alignItems: 'center', gap: 20, padding: '8px 16px', background: '#0f0f23', borderBottom: '2px solid #3a3520', flexShrink: 0 },
  title: { fontWeight: 'bold', fontSize: 18, color: '#daa520' },
  screenTabs: { display: 'flex', background: '#0f0f23', borderBottom: '2px solid #3a3520', flexShrink: 0 },
  screenTab: { flex: 1, padding: '10px 8px', textAlign: 'center' as const, fontSize: 13, color: '#9a9a9a', background: 'none', border: 'none', cursor: 'pointer', fontFamily: 'Georgia, serif', borderBottom: '3px solid transparent', display: 'flex', flexDirection: 'column' as const, alignItems: 'center' as const },
  screenTabActive: { color: '#daa520', borderBottomColor: '#daa520', background: 'rgba(218,165,32,0.05)' },
  hotkey: { fontSize: 10, color: '#555', display: 'block', marginTop: 2 },
  hotkeyActive: { fontSize: 10, color: '#8a7530', display: 'block', marginTop: 2 },
  mainArea: { display: 'flex', flex: 1, overflow: 'hidden', minHeight: 0 },
  mapContainer: { flex: 1, background: '#0a0a1a', minHeight: 0, position: 'relative' as const },
  sidePanel: { width: 260, padding: 12, background: '#161625', borderLeft: '2px solid #3a3520', overflowY: 'auto' as const, flexShrink: 0 },
  panelTitle: { margin: '12px 0 6px', color: '#daa520', borderBottom: '1px solid #3a3520', paddingBottom: 4 },
  tileInfo: { fontSize: 13 },
  tileSelected: { background: 'rgba(218,165,32,0.1)', border: '1px solid rgba(218,165,32,0.3)', borderRadius: 4, padding: 8, marginBottom: 8 },
  tileHovered: { padding: 8, marginBottom: 8, opacity: 0.8 },
  tileLabel: { fontSize: 11, color: '#daa520', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  tileLabelDim: { fontSize: 11, color: '#888', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  hint: { color: '#9a9a9a', fontStyle: 'italic' },
  nationList: { fontSize: 13 },
  nationItem: { display: 'flex', justifyContent: 'space-between', padding: '2px 0' },
  btn: { padding: '4px 12px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', cursor: 'pointer', fontFamily: 'Georgia, serif' },
  endTurnBtn: { padding: '6px 20px', background: '#8b4513', color: '#fff', border: '1px solid #a0522d', cursor: 'pointer', fontWeight: 'bold', fontFamily: 'Georgia, serif' },
  modal: { position: 'fixed' as const, inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', justifyContent: 'center', alignItems: 'center', zIndex: 100 },
  modalContent: { background: '#1a1a2e', border: '2px solid #daa520', padding: 24, maxWidth: 500, maxHeight: '80vh', overflowY: 'auto' as const },
  newspaperModal: { background: '#1a1a2e', border: '2px solid #daa520', width: 540, maxHeight: '80vh', overflowY: 'auto' as const },
  masthead: { padding: '20px 24px 16px', textAlign: 'center' as const, borderBottom: '3px double #daa520', background: 'linear-gradient(180deg, #1e1e35 0%, #1a1a2e 100%)' },
  newspaperTitle: { fontFamily: "'Times New Roman', serif", textAlign: 'center' as const, color: '#daa520', margin: 0, fontSize: 28 },
  mastheadDate: { fontSize: 13, color: '#9a9a9a', marginTop: 4 },
  newsBody: { padding: '16px 24px 20px' },
  sectionLabelPlayer: { fontSize: 11, textTransform: 'uppercase' as const, letterSpacing: 1, padding: '4px 0', marginBottom: 8, borderBottom: '1px solid #5a4a20', color: '#daa520' },
  sectionLabelWorld: { fontSize: 11, textTransform: 'uppercase' as const, letterSpacing: 1, padding: '4px 0', marginBottom: 8, borderBottom: '1px solid #3a3520', color: '#666' },
  headlineRow: { padding: '5px 0 5px 12px', margin: '3px 0', fontSize: 13, borderLeft: '3px solid transparent', lineHeight: '1.4' },
  nationTag: { fontSize: 10, fontWeight: 'bold' as const, textTransform: 'uppercase' as const, letterSpacing: 0.5, marginRight: 6, opacity: 0.7 },
  newsFooter: { padding: '12px 24px', borderTop: '1px solid #3a3520', display: 'flex', justifyContent: 'space-between', alignItems: 'center' },
  headline: { margin: '6px 0', fontSize: 14 },
  techItem: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '4px 0' },
};

export default App;
