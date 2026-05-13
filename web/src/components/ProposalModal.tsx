import type { PendingProposal } from '../wasm';

interface Props {
  proposals: PendingProposal[];
  onAccept: (index: number) => void;
  onReject: (index: number) => void;
  onClose: () => void;
}

export default function ProposalModal({ proposals, onAccept, onReject, onClose }: Props) {
  if (proposals.length === 0) return null;

  return (
    <div style={backdropStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <div style={headerStyle}>
          <h3 style={{ margin: 0, color: '#daa520', fontSize: 16 }}>Diplomatic Proposals</h3>
        </div>

        <div style={{ padding: '12px 16px', maxHeight: '60vh', overflowY: 'auto' }}>
          {proposals.map(p => {
            const isWarDeclaration = p.proposal_type === 'WarDeclaration';
            return (
              <div key={p.index} style={proposalRowStyle}>
                <div style={{ marginBottom: 6 }}>
                  <span style={{ fontWeight: 'bold', color: '#e0d8c0' }}>{p.display_text}</span>
                  <span style={{ fontSize: 11, color: '#888', marginLeft: 8 }}>
                    (expires in {p.turns_until_expiry} turn{p.turns_until_expiry !== 1 ? 's' : ''})
                  </span>
                </div>
                <div style={{ display: 'flex', gap: 6 }}>
                  <button onClick={() => onAccept(p.index)} style={acceptBtnStyle}>
                    {isWarDeclaration ? 'Acknowledge' : 'Accept'}
                  </button>
                  {!isWarDeclaration && (
                    <button onClick={() => onReject(p.index)} style={rejectBtnStyle}>Reject</button>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        <div style={{ padding: '8px 16px', borderTop: '1px solid #3a3520', textAlign: 'right' }}>
          <button onClick={onClose} style={dismissBtnStyle}>Dismiss</button>
        </div>
      </div>
    </div>
  );
}

const backdropStyle: React.CSSProperties = {
  position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)',
  display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000,
};

const modalStyle: React.CSSProperties = {
  background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 8,
  maxWidth: 500, width: '90%', color: '#e0d8c0',
};

const headerStyle: React.CSSProperties = {
  padding: '12px 16px', borderBottom: '1px solid #3a3520',
};

const proposalRowStyle: React.CSSProperties = {
  background: 'rgba(255,255,255,0.03)', borderRadius: 4,
  padding: '8px 10px', marginBottom: 8, fontSize: 13,
};

const acceptBtnStyle: React.CSSProperties = {
  background: '#2a6', color: '#fff', border: 'none', borderRadius: 3,
  padding: '4px 12px', fontSize: 12, cursor: 'pointer',
};

const rejectBtnStyle: React.CSSProperties = {
  background: '#a33', color: '#fff', border: 'none', borderRadius: 3,
  padding: '4px 12px', fontSize: 12, cursor: 'pointer',
};

const dismissBtnStyle: React.CSSProperties = {
  background: '#3a3520', color: '#e0d8c0', border: 'none', borderRadius: 3,
  padding: '4px 14px', fontSize: 12, cursor: 'pointer',
};
