import React, { useState } from 'react';
import type { Headline, ArchivedNewspaper } from '../wasm';

const CATEGORY_COLORS: Record<string, string> = {
  war:       '#e63946',
  battle:    '#e76f51',
  diplomacy: '#457b9d',
  growth:    '#2a9d8f',
  trade:     '#daa520',
  crisis:    '#9d0208',
  politics:  '#b380e6',
  military:  '#8a9aaf',
  default:   '#333',
};

const NEWS_CATEGORY_OPTIONS: { value: string; label: string }[] = [
  { value: 'all',       label: 'All topics' },
  { value: 'war',       label: 'War' },
  { value: 'battle',    label: 'Battle' },
  { value: 'diplomacy', label: 'Diplomacy' },
  { value: 'growth',    label: 'Growth' },
  { value: 'trade',     label: 'Trade' },
  { value: 'crisis',    label: 'Crisis' },
  { value: 'politics',  label: 'Politics' },
  { value: 'military',  label: 'Military' },
  { value: 'other',     label: 'Other' },
];

function applyNewsFilters(
  headlines: Headline[],
  opts: { showNonActions: boolean; category: string; country: string },
): Headline[] {
  return headlines.filter(h => {
    if (!opts.showNonActions && h.is_non_action) return false;
    if (opts.category !== 'all' && (h.category || 'default') !== opts.category) return false;
    if (opts.country !== 'all' && !h.text.includes(opts.country)) return false;
    return true;
  });
}

function extractNationTag(text: string, nations?: any[]): string | null {
  if (!nations) return null;
  for (const n of nations) {
    if (n.nation_type === 'GreatPower' && text.includes(n.name)) return n.name;
  }
  return null;
}

interface Props {
  playerName: string;
  year: number;
  quarter: number;
  turnNumber: number;
  headlines: Headline[];
  playerNews: Headline[];
  worldNews: Headline[];
  archiveData: ArchivedNewspaper[];
  nations: any[];
  countryOptions: string[];
  newsFilterCategory: string;
  newsFilterCountry: string;
  showAiReasoning: boolean;
  showAiNonActions: boolean;
  onCategoryChange: (cat: string) => void;
  onCountryChange: (country: string) => void;
  onDismiss: () => void;
  onClose: () => void;
}

export default function NewspaperScreen({
  playerName, year, quarter, turnNumber,
  headlines, archiveData, nations, countryOptions,
  newsFilterCategory, newsFilterCountry,
  showAiReasoning, showAiNonActions,
  onCategoryChange, onCountryChange,
  onDismiss, onClose,
}: Props) {
  const [mode, setMode] = useState<'current' | 'archive'>('current');
  const [selectedArchiveTurn, setSelectedArchiveTurn] = useState<number | null>(null);
  const [localCategory, setLocalCategory] = useState(newsFilterCategory);
  const [localCountry, setLocalCountry] = useState(newsFilterCountry);

  const handleCategoryChange = (cat: string) => { setLocalCategory(cat); onCategoryChange(cat); };
  const handleCountryChange = (country: string) => { setLocalCountry(country); onCountryChange(country); };

  // Get current headlines to display
  const currentHeadlines = mode === 'current' ? headlines : (() => {
    const entry = archiveData.find(a => a.turn === selectedArchiveTurn);
    return entry?.headlines || [];
  })();

  const visible = applyNewsFilters(currentHeadlines, {
    showNonActions: showAiNonActions,
    category: localCategory,
    country: localCountry,
  });
  const playerNews = visible.filter(h => h.text.includes(playerName));
  const worldNews = visible.filter(h => !h.text.includes(playerName));

  // Sort archive most recent first
  const sortedArchive = [...archiveData].sort((a, b) => b.turn - a.turn);

  const displayYear = mode === 'archive' && selectedArchiveTurn !== null
    ? (() => { const e = archiveData.find(a => a.turn === selectedArchiveTurn); return e ? e.year : year; })()
    : year;
  const displayQuarter = mode === 'archive' && selectedArchiveTurn !== null
    ? (() => { const e = archiveData.find(a => a.turn === selectedArchiveTurn); return e ? e.quarter : quarter; })()
    : quarter;
  const displayTurn = mode === 'archive' && selectedArchiveTurn !== null ? selectedArchiveTurn : turnNumber;

  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        {/* Masthead */}
        <div style={styles.masthead}>
          <h2 style={styles.title}>The Imperial Times</h2>
          <div style={styles.date}>{displayYear} Q{displayQuarter} — Turn {displayTurn}</div>
        </div>

        {/* Mode tabs + filters */}
        <div style={styles.toolbar}>
          <div style={styles.modeTabs}>
            <button
              style={mode === 'current' ? { ...styles.modeTab, ...styles.modeTabActive } : styles.modeTab}
              onClick={() => setMode('current')}
            >
              Current
            </button>
            <button
              style={mode === 'archive' ? { ...styles.modeTab, ...styles.modeTabActive } : styles.modeTab}
              onClick={() => { setMode('archive'); setArchiveData(); }}
            >
              Archive ({archiveData.length})
            </button>
          </div>
          <div style={styles.filters}>
            <select value={localCategory} onChange={e => handleCategoryChange(e.target.value)} style={styles.select}>
              {NEWS_CATEGORY_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            <select value={localCountry} onChange={e => handleCountryChange(e.target.value)} style={styles.select}>
              <option value="all">All countries</option>
              {countryOptions.map(n => <option key={n} value={n}>{n}</option>)}
            </select>
          </div>
        </div>

        {/* Content area */}
        <div style={styles.contentArea}>
          {/* Archive sidebar */}
          {mode === 'archive' && (
            <div style={styles.archiveSidebar}>
              {sortedArchive.length === 0 && <div style={{ padding: 12, color: '#666' }}>No reports yet</div>}
              {sortedArchive.map(entry => (
                <div
                  key={entry.turn}
                  onClick={() => setSelectedArchiveTurn(entry.turn)}
                  style={{
                    padding: '8px 12px', cursor: 'pointer', fontSize: 13,
                    background: selectedArchiveTurn === entry.turn ? 'rgba(0,0,0,0.1)' : 'transparent',
                    borderLeft: selectedArchiveTurn === entry.turn ? '3px solid #8b4513' : '3px solid transparent',
                    color: selectedArchiveTurn === entry.turn ? '#333' : '#666',
                    fontWeight: selectedArchiveTurn === entry.turn ? 'bold' : 'normal',
                  }}
                >
                  Turn {entry.turn} ({entry.year} Q{entry.quarter})
                </div>
              ))}
            </div>
          )}

          {/* Headlines */}
          <div style={styles.headlinesArea}>
            {mode === 'archive' && selectedArchiveTurn === null && (
              <div style={{ color: '#999', fontStyle: 'italic', padding: 20 }}>Select a turn from the sidebar to view its headlines.</div>
            )}
            {(mode === 'current' || selectedArchiveTurn !== null) && playerNews.length === 0 && worldNews.length === 0 && (
              <div style={{ padding: 20, color: '#999', fontStyle: 'italic' }}>No headlines match the current filters.</div>
            )}
            {playerNews.length > 0 && (
              <div style={{ marginBottom: 24 }}>
                <div style={styles.sectionLabel}>Your Empire — {playerName}</div>
                {playerNews.map((h, i) => (
                  <div key={i} style={{
                    ...styles.headline,
                    borderLeftColor: CATEGORY_COLORS[h.category || 'default'] || CATEGORY_COLORS.default,
                  }}>
                    {h.text}
                    {showAiReasoning && h.reason && (
                      <div style={{ fontSize: 12, color: '#888', marginTop: 2, fontStyle: 'italic' }}>{h.reason}</div>
                    )}
                  </div>
                ))}
              </div>
            )}
            {worldNews.length > 0 && (
              <div>
                <div style={{ ...styles.sectionLabel, color: '#666', borderBottomColor: '#ddd' }}>World News</div>
                {worldNews.map((h, i) => {
                  const tag = extractNationTag(h.text, nations);
                  return (
                    <div key={i} style={{
                      ...styles.headline,
                      borderLeftColor: CATEGORY_COLORS[h.category || 'default'] || CATEGORY_COLORS.default,
                    }}>
                      {tag && <span style={styles.nationTag}>{tag}</span>}
                      {h.text}
                      {showAiReasoning && h.reason && (
                        <div style={{ fontSize: 12, color: '#888', marginTop: 2, fontStyle: 'italic' }}>{h.reason}</div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div style={styles.footer}>
          <button onClick={mode === 'current' ? onDismiss : onClose} style={styles.closeBtn}>Back to Map</button>
          {mode === 'current' && (
            <button onClick={onDismiss} style={styles.continueBtn}>Continue</button>
          )}
        </div>
      </div>
    </div>
  );

  function setArchiveData() {
    // Archive data is passed in as props; no action needed
  }
}

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    flex: 1, minHeight: 0,
    background: '#faf5e8',
    display: 'flex', flexDirection: 'column',
    fontFamily: "'Georgia', 'Times New Roman', serif",
    color: '#1a1a1a',
  },
  container: {
    display: 'flex', flexDirection: 'column', height: '100%',
  },
  masthead: {
    padding: '24px 32px 16px', textAlign: 'center',
    borderBottom: '3px double #333',
    background: '#faf5e8',
  },
  title: {
    fontFamily: "'Times New Roman', 'Georgia', serif",
    fontSize: 36, fontWeight: 'bold', margin: 0, color: '#1a1a1a',
    letterSpacing: 2,
  },
  date: {
    fontSize: 14, color: '#666', marginTop: 4,
  },
  toolbar: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '8px 24px', borderBottom: '1px solid #ccc',
    background: '#f0ead6',
  },
  modeTabs: {
    display: 'flex', gap: 4,
  },
  modeTab: {
    padding: '6px 16px', background: 'transparent', border: '1px solid #999',
    cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 13, color: '#666',
    borderRadius: 3,
  },
  modeTabActive: {
    background: '#8b4513', color: '#fff', borderColor: '#8b4513',
  },
  filters: {
    display: 'flex', gap: 8,
  },
  select: {
    padding: '4px 8px', fontFamily: "'Georgia', serif", fontSize: 12,
    border: '1px solid #999', background: '#fff', color: '#333',
  },
  contentArea: {
    display: 'flex', flex: 1, minHeight: 0, overflow: 'hidden',
  },
  archiveSidebar: {
    width: 180, borderRight: '1px solid #ccc', overflowY: 'auto' as const,
    padding: '8px 0', background: '#f5efe0',
  },
  headlinesArea: {
    flex: 1, overflowY: 'auto' as const, padding: '20px 32px',
    columnCount: 2, columnGap: 32, columnRule: '1px solid #ddd',
  },
  sectionLabel: {
    fontSize: 12, textTransform: 'uppercase' as const, letterSpacing: 1.5,
    padding: '4px 0', marginBottom: 12, borderBottom: '2px solid #333',
    color: '#333', fontWeight: 'bold',
    columnSpan: 'all' as any,
  },
  headline: {
    padding: '6px 0 6px 12px', margin: '4px 0', fontSize: 14,
    borderLeft: '3px solid transparent', lineHeight: 1.5,
    breakInside: 'avoid' as any,
  },
  nationTag: {
    fontSize: 10, fontWeight: 'bold', textTransform: 'uppercase' as const,
    letterSpacing: 0.5, marginRight: 6, color: '#8b4513',
  },
  footer: {
    padding: '12px 24px', borderTop: '2px solid #333',
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    background: '#f0ead6',
  },
  closeBtn: {
    padding: '6px 16px', background: '#666', color: '#fff',
    border: 'none', cursor: 'pointer', fontFamily: "'Georgia', serif", borderRadius: 3,
  },
  continueBtn: {
    padding: '8px 24px', background: '#8b4513', color: '#fff',
    border: 'none', cursor: 'pointer', fontWeight: 'bold',
    fontFamily: "'Georgia', serif", fontSize: 14, borderRadius: 3,
  },
};
