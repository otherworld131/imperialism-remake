interface Props {
  busy: boolean;
  message?: string;
}

export default function BusyOverlay({ busy, message }: Props) {
  if (!busy) return null;
  return (
    <div style={styles.overlay}>
      <div style={styles.spinner} />
      {message && <div style={styles.message}>{message}</div>}
      <style>{keyframes}</style>
    </div>
  );
}

const keyframes = `
@keyframes busy-spin {
  to { transform: rotate(360deg); }
}
`;

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    position: 'fixed',
    inset: 0,
    background: 'rgba(10,10,30,0.55)',
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 9999,
    pointerEvents: 'all',
  },
  spinner: {
    width: 48,
    height: 48,
    border: '4px solid rgba(218,165,32,0.2)',
    borderTopColor: '#daa520',
    borderRadius: '50%',
    animation: 'busy-spin 0.9s linear infinite',
  },
  message: {
    marginTop: 16,
    color: '#e0d8c0',
    fontFamily: 'Georgia, serif',
    fontSize: 14,
  },
};
