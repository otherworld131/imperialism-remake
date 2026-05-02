import type { TechScreenData, TechEntry } from '../wasm';

interface Props {
  data: TechScreenData;
  year: number;
  isObserver: boolean;
  onQueue: (techName: string) => void;
  onCancel: () => void;
  onClose: () => void;
}

export default function TechScreen({ data, year, isObserver, onQueue, onCancel, onClose }: Props) {
  const { available, researched, pending, treasury } = data;

  const researchedById = new Map(researched.map(t => [t.id, t]));
  const availableIds = new Set(available.map(t => t.id));
  const historicResearched = researched.filter(t => !availableIds.has(t.id));

  return (
    <div style={styles.page}>
      <div style={styles.header}>
        <span style={styles.title}>Technology — {year}</span>
        <button onClick={onClose} style={styles.closeBtn}>✕</button>
      </div>

      {pending && (
        <div style={styles.pendingBanner}>
          <span>
            Queued: <b>{pending.name}</b>
            {pending.cost > 0 && <span style={{ color: '#ffd900' }}> (${pending.cost.toLocaleString()})</span>}
            {pending.description && <span style={styles.pendingDesc}> — {pending.description}</span>}
            <span style={{ color: '#888', fontSize: 12, marginLeft: 8 }}>researched at end of turn</span>
          </span>
          {!isObserver && (
            <button onClick={onCancel} style={styles.cancelBtn}>Cancel</button>
          )}
        </div>
      )}

      <div style={styles.tableWrap}>
        <table style={styles.table}>
          <colgroup>
            <col style={{ width: '22%', minWidth: 180 }} />
            <col style={{ width: '100%' }} />
            <col style={{ width: 'auto', whiteSpace: 'nowrap' as const }} />
          </colgroup>
          <thead>
            <tr>
              <th style={styles.th}>Technology</th>
              <th style={styles.th}>Effect</th>
              <th style={{ ...styles.th, textAlign: 'right' }}>Purchase / Status</th>
            </tr>
          </thead>
          <tbody>
            {available.length === 0 && researched.length === 0 && (
              <tr><td colSpan={3} style={styles.empty}>No technologies available this year.</td></tr>
            )}

            {available.map(tech => {
              const isAlreadyResearched = researchedById.has(tech.id);
              if (isAlreadyResearched) {
                const entry = researchedById.get(tech.id)!;
                return (
                  <tr key={tech.id} style={styles.rowResearched}>
                    <td style={styles.techCell}>
                      <span style={styles.techNameDim}>{tech.name}</span>
                    </td>
                    <td style={styles.descCell}>{tech.description}</td>
                    <td style={styles.actionCell}>
                      <button disabled style={styles.purchasedBtn}>
                        ✓ {entry.year > 0 ? entry.year : 'Researched'}
                      </button>
                    </td>
                  </tr>
                );
              }
              const isQueued = pending?.id === tech.id;
              const canAfford = treasury >= tech.cost;
              return (
                <tr key={tech.id} style={isQueued ? styles.rowQueued : styles.rowAvailable}>
                  <td style={styles.techCell}>
                    <span style={styles.techName}>{tech.name}</span>
                    {tech.latest_year && tech.latest_year < 9999 && (
                      <span style={styles.yearRange}> {tech.earliest_year}–{tech.latest_year}</span>
                    )}
                  </td>
                  <td style={styles.descCell}>{tech.description}</td>
                  <td style={styles.actionCell}>
                    {isQueued ? (
                      <span style={styles.queuedLabel}>Queued ✓</span>
                    ) : (
                      <button
                        onClick={() => !isObserver && onQueue(tech.name)}
                        disabled={isObserver || !canAfford || pending != null}
                        style={canAfford && !pending && !isObserver ? styles.purchaseBtn : styles.purchaseBtnDisabled}
                        title={
                          !canAfford
                            ? `Insufficient funds (need $${tech.cost.toLocaleString()})`
                            : pending != null
                            ? 'Cancel the current queued tech first'
                            : undefined
                        }
                      >
                        {tech.cost > 0 ? `$${tech.cost.toLocaleString()}` : 'Free'}
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}

            {historicResearched.map(tech => (
              <tr key={tech.id} style={styles.rowResearched}>
                <td style={styles.techCell}>
                  <span style={styles.techNameDim}>{tech.name}</span>
                </td>
                <td style={styles.descCell}>{tech.description}</td>
                <td style={styles.actionCell}>
                  <button disabled style={styles.purchasedBtn}>
                    ✓ {tech.year > 0 ? tech.year : 'Researched'}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  page: {
    flex: 1,
    minHeight: 0,
    display: 'flex',
    flexDirection: 'column',
    background: '#161625',
    color: '#e0d8c0',
    fontFamily: "'Georgia', serif",
    fontSize: 'var(--ui-font-size, 14px)',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '12px 24px',
    borderBottom: '2px solid #3a3520',
    flexShrink: 0,
  },
  title: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#daa520',
  },
  closeBtn: {
    background: 'none',
    border: '1px solid #5a5030',
    color: '#e0d8c0',
    cursor: 'pointer',
    padding: '2px 10px',
    fontFamily: "'Georgia', serif",
    fontSize: 14,
  },
  pendingBanner: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    background: 'rgba(218,165,32,0.12)',
    border: '1px solid rgba(218,165,32,0.4)',
    padding: '10px 24px',
    fontSize: 'var(--ui-font-size, 14px)',
    flexShrink: 0,
  },
  pendingDesc: {
    color: '#aaa',
    fontSize: 12,
    fontStyle: 'italic',
  },
  cancelBtn: {
    padding: '4px 12px',
    background: '#3a3520',
    color: '#e0d8c0',
    border: '1px solid #5a5030',
    cursor: 'pointer',
    fontFamily: "'Georgia', serif",
    fontSize: 'var(--ui-font-size, 14px)',
    marginLeft: 16,
    flexShrink: 0,
  },
  tableWrap: {
    flex: 1,
    overflowY: 'auto',
    padding: '0 24px 24px',
  },
  table: {
    width: '100%',
    borderCollapse: 'collapse',
  },
  th: {
    position: 'sticky',
    top: 0,
    background: '#161625',
    padding: '10px 0 6px',
    fontSize: 11,
    color: '#888',
    textTransform: 'uppercase' as const,
    letterSpacing: 0.5,
    borderBottom: '1px solid #3a3520',
    textAlign: 'left',
    fontWeight: 'normal',
    zIndex: 1,
  },
  rowAvailable: {
    borderBottom: '1px solid #222233',
  },
  rowQueued: {
    borderBottom: '1px solid #222233',
    background: 'rgba(218,165,32,0.06)',
  },
  rowResearched: {
    borderBottom: '1px solid #222233',
    opacity: 0.55,
  },
  techCell: {
    padding: '8px 16px 8px 0',
    whiteSpace: 'nowrap',
  },
  descCell: {
    padding: '8px 16px 8px 0',
    fontSize: 12,
    color: '#8a9aaa',
    fontStyle: 'italic',
  },
  techName: {
    fontSize: 'var(--ui-font-size, 14px)',
  },
  techNameDim: {
    fontSize: 'var(--ui-font-size, 14px)',
    color: '#9a9a9a',
  },
  yearRange: {
    fontSize: 11,
    color: '#666',
  },
  actionCell: {
    padding: '8px 0 8px 16px',
    textAlign: 'right',
    whiteSpace: 'nowrap',
    verticalAlign: 'top',
  },
  purchaseBtn: {
    padding: '4px 16px',
    background: '#3a5520',
    color: '#c8e8a0',
    border: '1px solid #4a7030',
    cursor: 'pointer',
    fontFamily: "'Georgia', serif",
    fontSize: 'var(--ui-font-size, 14px)',
    fontWeight: 'bold',
  },
  purchaseBtnDisabled: {
    padding: '4px 16px',
    background: '#2a2a3a',
    color: '#666',
    border: '1px solid #3a3a4a',
    cursor: 'not-allowed',
    fontFamily: "'Georgia', serif",
    fontSize: 'var(--ui-font-size, 14px)',
  },
  purchasedBtn: {
    padding: '4px 12px',
    background: 'transparent',
    color: '#5a7a5a',
    border: '1px solid #3a5a3a',
    cursor: 'default',
    fontFamily: "'Georgia', serif",
    fontSize: 12,
  },
  queuedLabel: {
    color: '#daa520',
    fontSize: 12,
    fontStyle: 'italic',
  },
  empty: {
    color: '#9a9a9a',
    fontStyle: 'italic',
    padding: '24px 0',
    textAlign: 'center',
  },
};
