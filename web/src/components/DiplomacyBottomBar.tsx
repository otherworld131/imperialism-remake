import { useState, type CSSProperties } from 'react';
import type { DiplomacyScreenData, DiplomacyScreenRelation } from '../wasm';

export type QueuedDiplomacyAction =
  | { kind: 'consulate' }
  | { kind: 'embassy' }
  | { kind: 'nap' }
  | { kind: 'alliance' }
  | { kind: 'peace' }
  | { kind: 'grant'; amount: number }
  | { kind: 'breakTreaty'; treatyType: string }
  | { kind: 'war' };

interface Props {
  diplomacy: DiplomacyScreenData;
  hoveredNationId: number | null;
  playerNationId: number;
  playerStanding: number;
  queuedAction: QueuedDiplomacyAction | null;
  onQueue: (action: QueuedDiplomacyAction | null) => void;
  onBuildConsulate: (targetId: number) => void;
  onBuildEmbassy: (targetId: number) => void;
  onProposeNap: (targetId: number) => void;
  onProposeAlliance: (targetId: number) => void;
  onDeclareWar: (targetId: number) => void;
  onSendGrant: (targetId: number, amount: number) => void;
  onBreakTreaty: (targetId: number, treatyType: string) => void;
  onProposePeace: (targetId: number) => void;
}

const STATUS_COLORS: Record<string, string> = {
  'At War': '#e44',
  'Anarchy': '#a0a',
  'Alliance': '#4a4',
  'NAP': '#4aa',
  'Neutral': '#888',
};

const ACTION_LABELS: Record<string, string> = {
  consulate: '🏛️ Consulate',
  embassy: '🏰 Embassy',
  nap: '🤝 NAP',
  alliance: '🛡️ Alliance',
  peace: '🕊️ Peace',
  grant: '💰 Grant',
  breakTreaty: '📜 Break Treaty',
  war: '⚔️ Declare War',
};

export default function DiplomacyBottomBar({
  diplomacy,
  hoveredNationId,
  playerStanding,
  queuedAction,
  onQueue,
  onBuildConsulate,
  onBuildEmbassy,
  onProposeNap,
  onProposeAlliance,
  onDeclareWar,
  onSendGrant,
  onBreakTreaty,
  onProposePeace,
}: Props) {
  const [showGrantPicker, setShowGrantPicker] = useState(false);
  const [showBreakPicker, setShowBreakPicker] = useState(false);
  const [confirmWar, setConfirmWar] = useState(false);

  const focusedId = hoveredNationId;
  const rel: DiplomacyScreenRelation | undefined = focusedId != null
    ? diplomacy.relations.find(r => r.nation_id === focusedId)
    : undefined;

  const standingColor = playerStanding > 60 ? '#4a4' : playerStanding > 30 ? '#ca4' : '#e44';
  const standingPct = Math.max(0, Math.min(100, playerStanding));

  const queuedLabel = queuedAction ? ACTION_LABELS[queuedAction.kind] : null;

  function fireAction(action: QueuedDiplomacyAction) {
    if (focusedId == null || rel == null) {
      onQueue(action);
    } else {
      dispatchAction(action, focusedId);
    }
  }

  function dispatchAction(action: QueuedDiplomacyAction, targetId: number) {
    switch (action.kind) {
      case 'consulate': onBuildConsulate(targetId); break;
      case 'embassy': onBuildEmbassy(targetId); break;
      case 'nap': onProposeNap(targetId); break;
      case 'alliance': onProposeAlliance(targetId); break;
      case 'peace': onProposePeace(targetId); break;
      case 'grant': onSendGrant(targetId, action.amount); break;
      case 'breakTreaty': onBreakTreaty(targetId, action.treatyType); break;
      case 'war': onDeclareWar(targetId); break;
    }
  }

  const a = rel?.actions;
  const isAnarchy = rel?.is_in_anarchy ?? false;

  return (
    <div style={{
      position: 'absolute',
      bottom: 0,
      left: 0,
      right: 0,
      background: '#161625',
      borderTop: '2px solid #3a3520',
      zIndex: 10,
      padding: '6px 12px',
      display: 'flex',
      flexDirection: 'column',
      gap: 4,
    }}>
      {/* Top row: standing + focused nation info */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 12 }}>
        {/* Player standing */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 140 }}>
          <span style={{ color: '#888', whiteSpace: 'nowrap' }}>Standing:</span>
          <div style={{ flex: 1, height: 5, background: 'rgba(255,255,255,0.1)', borderRadius: 3, overflow: 'hidden', minWidth: 60 }}>
            <div style={{ width: `${standingPct}%`, height: '100%', background: standingColor, borderRadius: 3 }} />
          </div>
          <span style={{ color: standingColor, minWidth: 22 }}>{playerStanding}</span>
        </div>

        {/* Focused nation info */}
        {rel ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, flexWrap: 'wrap' }}>
            <span style={{ fontWeight: 'bold', color: '#e0d8c0' }}>{rel.nation_name}</span>
            <span style={{
              fontSize: 10, padding: '1px 5px', borderRadius: 3,
              background: `${STATUS_COLORS[rel.status] || '#555'}22`,
              color: STATUS_COLORS[rel.status] || '#888',
              border: `1px solid ${STATUS_COLORS[rel.status] || '#555'}`,
            }}>{rel.status}</span>
            <span style={{ color: rel.score > 0 ? '#4a4' : rel.score < 0 ? '#e44' : '#888' }}>
              {rel.score >= 0 ? '+' : ''}{rel.score}
            </span>
            {rel.treaties.map(t => (
              <span key={t} style={{ fontSize: 10, background: 'rgba(218,165,32,0.2)', color: '#daa520', borderRadius: 2, padding: '0 3px' }}>{t}</span>
            ))}
            {rel.has_embassy && <span style={{ fontSize: 10, color: '#888' }}>Embassy</span>}
            {rel.has_consulate && !rel.has_embassy && <span style={{ fontSize: 10, color: '#888' }}>Consulate</span>}
            {isAnarchy && <span style={{ fontSize: 10, color: '#a0a', fontStyle: 'italic' }}>In anarchy</span>}
          </div>
        ) : (
          <div style={{ flex: 1, color: '#888', fontStyle: 'italic', fontSize: 12 }}>
            {queuedLabel
              ? `🎯 ${queuedLabel} queued — click a nation on the map (Esc to cancel)`
              : 'Hover over a nation on the map, or click an action to queue it'}
          </div>
        )}
      </div>

      {/* Action buttons row */}
      <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap', alignItems: 'center' }}>
        <ActionBtn
          label={`🏛️ Consulate${a ? ` $${a.consulate_cost}` : ''}`}
          active={queuedAction?.kind === 'consulate'}
          disabled={isAnarchy || (a ? !a.can_build_consulate : false)}
          onClick={() => { fireAction({ kind: 'consulate' }); setShowGrantPicker(false); setShowBreakPicker(false); setConfirmWar(false); }}
        />
        <ActionBtn
          label={`🏰 Embassy${a ? ` $${a.embassy_cost}` : ''}`}
          active={queuedAction?.kind === 'embassy'}
          disabled={isAnarchy || (a ? !a.can_build_embassy : false)}
          onClick={() => { fireAction({ kind: 'embassy' }); setShowGrantPicker(false); setShowBreakPicker(false); setConfirmWar(false); }}
        />
        <ActionBtn
          label="🤝 NAP"
          active={queuedAction?.kind === 'nap'}
          disabled={isAnarchy || (a ? !a.can_propose_nap : false)}
          onClick={() => { fireAction({ kind: 'nap' }); setShowGrantPicker(false); setShowBreakPicker(false); setConfirmWar(false); }}
        />
        <ActionBtn
          label="🛡️ Alliance"
          active={queuedAction?.kind === 'alliance'}
          disabled={isAnarchy || (a ? !a.can_propose_alliance : false)}
          onClick={() => { fireAction({ kind: 'alliance' }); setShowGrantPicker(false); setShowBreakPicker(false); setConfirmWar(false); }}
        />
        <ActionBtn
          label="🕊️ Peace"
          active={queuedAction?.kind === 'peace'}
          disabled={isAnarchy || (a ? !a.can_propose_peace : false)}
          onClick={() => { fireAction({ kind: 'peace' }); setShowGrantPicker(false); setShowBreakPicker(false); setConfirmWar(false); }}
        />

        {/* Grant */}
        <div style={{ position: 'relative' }}>
          <ActionBtn
            label="💰 Grant"
            active={queuedAction?.kind === 'grant' || showGrantPicker}
            disabled={isAnarchy || (a ? !a.can_send_grant : false)}
            onClick={() => { setShowGrantPicker(p => !p); setShowBreakPicker(false); setConfirmWar(false); }}
          />
          {showGrantPicker && (
            <div style={{
              position: 'absolute', bottom: '100%', left: 0, background: '#161625',
              border: '1px solid #3a3520', borderRadius: 3, padding: 4, display: 'flex', gap: 3, zIndex: 20,
            }}>
              {[500, 1000, 2000, 5000].map(amt => (
                <button key={amt} onClick={() => { fireAction({ kind: 'grant', amount: amt }); setShowGrantPicker(false); }}
                  style={btnStyle('#456')}>
                  ${amt}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Break Treaty */}
        {(a?.can_break_treaty || !a) && (
          <div style={{ position: 'relative' }}>
            <ActionBtn
              label="📜 Break Treaty"
              active={queuedAction?.kind === 'breakTreaty' || showBreakPicker}
              disabled={isAnarchy || (a ? !a.can_break_treaty : false)}
              onClick={() => { setShowBreakPicker(p => !p); setShowGrantPicker(false); setConfirmWar(false); }}
            />
            {showBreakPicker && a && a.breakable_treaties.length > 0 && (
              <div style={{
                position: 'absolute', bottom: '100%', left: 0, background: '#161625',
                border: '1px solid #3a3520', borderRadius: 3, padding: 4, display: 'flex', gap: 3, zIndex: 20,
              }}>
                {a.breakable_treaties.map(t => (
                  <button key={t} onClick={() => { fireAction({ kind: 'breakTreaty', treatyType: t }); setShowBreakPicker(false); }}
                    style={btnStyle('#a63')}>
                    {t}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Declare War */}
        {!confirmWar ? (
          <ActionBtn
            label="⚔️ Declare War"
            active={queuedAction?.kind === 'war'}
            disabled={isAnarchy || (a ? !a.can_declare_war : false)}
            color="#a33"
            onClick={() => { setConfirmWar(true); setShowGrantPicker(false); setShowBreakPicker(false); }}
          />
        ) : (
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <span style={{ fontSize: 11, color: '#e44' }}>Breaks all treaties. Sure?</span>
            <ActionBtn label="Confirm ⚔️" color="#a33"
              onClick={() => { fireAction({ kind: 'war' }); setConfirmWar(false); }} />
            <ActionBtn label="Cancel" color="#555"
              onClick={() => setConfirmWar(false)} />
          </div>
        )}
      </div>
    </div>
  );
}

function btnStyle(bg: string): CSSProperties {
  return {
    background: bg, color: '#e0d8c0', border: 'none',
    borderRadius: 2, padding: '2px 7px', fontSize: 11,
    cursor: 'pointer', fontFamily: 'Georgia, serif',
  };
}

function ActionBtn({ label, onClick, color, disabled, active }: {
  label: string;
  onClick: () => void;
  color?: string;
  disabled?: boolean;
  active?: boolean;
}) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); if (!disabled) onClick(); }}
      disabled={disabled}
      style={{
        background: active ? (color || '#5a4530') : (color || '#3a3520'),
        color: '#e0d8c0', border: active ? '1px solid #daa520' : '1px solid transparent',
        borderRadius: 3, padding: '3px 8px', fontSize: 12,
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.4 : 1,
        fontFamily: 'Georgia, serif',
        whiteSpace: 'nowrap',
        outline: active ? '1px solid #daa52066' : 'none',
      }}
    >
      {label}
    </button>
  );
}
