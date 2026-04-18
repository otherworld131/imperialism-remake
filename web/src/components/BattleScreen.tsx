import React from 'react';

interface Props {
  onClose: () => void;
}

export default function BattleScreen({ onClose }: Props) {
  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        <div style={styles.header}>
          <h2 style={styles.title}>Battles</h2>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>
        <div style={styles.body}>
          <p style={{ color: '#999', fontStyle: 'italic', padding: 24 }}>
            No battles to display. Battles will appear here after combat occurs during turn resolution.
          </p>
        </div>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    flex: 1, minHeight: 0,
    background: '#1a1a2e', color: '#e0d8c0',
    display: 'flex', flexDirection: 'column',
    fontFamily: "'Georgia', serif",
  },
  container: {
    display: 'flex', flexDirection: 'column', height: '100%',
  },
  header: {
    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
    padding: '12px 24px', borderBottom: '2px solid #3a3520',
    background: '#0f0f23',
  },
  title: { color: '#daa520', margin: 0, fontSize: 22 },
  closeBtn: {
    padding: '4px 12px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', cursor: 'pointer', fontFamily: "'Georgia', serif",
  },
  body: {
    flex: 1, overflow: 'auto',
  },
};
