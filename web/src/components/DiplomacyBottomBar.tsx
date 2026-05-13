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
  selectedNationId: number | null;
  playerNationId: number | null;
  playerStanding: number;
  queuedAction: QueuedDiplomacyAction | null;
  onQueue: (action: QueuedDiplomacyAction | null) => void;
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
  selectedNationId,
  playerNationId,
  playerStanding,
  queuedAction,
  onQueue,
}: Props) {
  const [showGrantPicker, setShowGrantPicker] = useState(false);
  const [showBreakPicker, setShowBreakPicker] = useState(false);
  const [confirmWar, setConfirmWar] = useState(false);

  // Focused nation: hover (transient) takes priority over selected (sticky).
  // Selected pins after click so info doesn't disappear when mouse leaves the map.
  const focusedNationId = hoveredNationId ?? (
    selectedNationId != null && selectedNationId !== playerNationId ? selectedNationId : null
  );
  const rel: DiplomacyScreenRelation | undefined = focusedNationId != null
    ? diplomacy.relations.find(r => r.nation_id === focusedNationId)
    : undefined;

  const standingColor = playerStanding > 60 ? '#4a4' : playerStanding > 30 ? '#ca4' : '#e44';
  const standingPct = Math.max(0, Math.min(100, playerStanding));

  const queuedLabel = queuedAction ? ACTION_LABELS[queuedAction.kind] : null;

  function queueAction(action: QueuedDiplomacyAction) {
    onQueue(action);
  }

  function clearAllPickers() {
    setShowGrantPicker(false);
    setShowBreakPicker(false);
    setConfirmWar(false);
  }

  const a = rel?.actions;
  const isAnarchy = rel?.is_in_anarchy ?? false;
  const pendingLabels = rel ? [
    rel.has_pending_consulate ? 'Consulate' : null,
    rel.has_pending_embassy ? 'Embassy' : null,
    rel.has_pending_nap ? 'NAP' : null,
    rel.has_pending_alliance ? 'Alliance' : null,
    rel.has_pending_peace ? 'Peace' : null,
    rel.pending_grant_amount_dollars != null ? `Grant $${rel.pending_grant_amount_dollars}` : null,
    ...rel.pending_break_treaties.map(t => `Break ${t}`),
    rel.has_pending_war ? 'War' : null,
  ].filter((label): label is string => Boolean(label)) : [];
  const hasPending = pendingLabels.length > 0;

  return (
    <div style={{
      flexShrink: 0,
      background: '#161625',
      borderTop: '2px solid #3a3520',
      padding: '10px 16px 12px',
      display: 'flex',
      flexDirection: 'column',
      gap: 8,
    }}>
      {/* Top row: standing + focused nation info + queued banner */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, fontSize: 13, minHeight: 28 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 200 }}>
          <span style={{ color: '#888', whiteSpace: 'nowrap' }}>Player Standing</span>
          <div style={{ flex: 1, height: 8, background: 'rgba(255,255,255,0.1)', borderRadius: 4, overflow: 'hidden', minWidth: 80 }}>
            <div style={{ width: `${standingPct}%`, height: '100%', background: standingColor, borderRadius: 4 }} />
          </div>
          <span style={{ color: standingColor, minWidth: 28, fontWeight: 'bold' }}>{playerStanding}</span>
        </div>

        {rel ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1, flexWrap: 'wrap' }}>
            <span style={{ fontWeight: 'bold', color: '#e0d8c0', fontSize: 15 }}>{rel.nation_name}</span>
            <span style={{
              fontSize: 11, padding: '2px 7px', borderRadius: 4,
              background: `${STATUS_COLORS[rel.status] || '#555'}33`,
              color: STATUS_COLORS[rel.status] || '#888',
              border: `1px solid ${STATUS_COLORS[rel.status] || '#555'}`,
              fontWeight: 'bold',
            }}>{rel.status}</span>
            <span style={{ color: rel.score > 0 ? '#4a4' : rel.score < 0 ? '#e44' : '#888', fontWeight: 'bold' }}>
              {rel.score >= 0 ? '+' : ''}{rel.score}
            </span>
            {rel.treaties.map(t => (
              <span key={t} style={{ fontSize: 11, background: 'rgba(218,165,32,0.2)', color: '#daa520', borderRadius: 3, padding: '1px 5px' }}>{t}</span>
            ))}
            {rel.has_embassy && <span style={{ fontSize: 11, color: '#aaa' }}>🏰 Embassy</span>}
            {rel.has_consulate && !rel.has_embassy && <span style={{ fontSize: 11, color: '#aaa' }}>🏛️ Consulate</span>}
            {hasPending && (
              <span style={{
                fontSize: 11, padding: '2px 6px', borderRadius: 3,
                background: 'rgba(218,165,32,0.25)', color: '#daa520',
                border: '1px solid #daa520',
              }}>
                ⏳ Pending {pendingLabels.join(', ')}
              </span>
            )}
            {isAnarchy && <span style={{ fontSize: 11, color: '#a0a', fontStyle: 'italic' }}>In anarchy</span>}
          </div>
        ) : (
          <div style={{ flex: 1, color: '#888', fontStyle: 'italic' }}>
            Hover over a nation on the map, or click an action to queue it.
          </div>
        )}

        {queuedLabel && (
          <div style={{
            display: 'flex', alignItems: 'center', gap: 8,
            background: 'rgba(218,165,32,0.18)',
            border: '1px solid #daa520',
            borderRadius: 4, padding: '4px 10px',
          }}>
            <span style={{ color: '#daa520', fontWeight: 'bold' }}>🎯 {queuedLabel}</span>
            <span style={{ color: '#daa520', fontSize: 12 }}>— click a nation on the map</span>
            <button
              onClick={(e) => { e.stopPropagation(); onQueue(null); }}
              style={{
                background: 'transparent', border: '1px solid #daa520',
                color: '#daa520', borderRadius: 3, padding: '1px 7px',
                cursor: 'pointer', fontSize: 12, fontFamily: 'Georgia, serif',
              }}
              title="Cancel queued action (Esc)"
            >
              ✕
            </button>
          </div>
        )}
      </div>

      {/* Action buttons row — centered, larger */}
      <div style={{ display: 'flex', gap: 8, justifyContent: 'center', alignItems: 'center', flexWrap: 'wrap' }}>
        <ActionBtn
          label="🏛️ Consulate"
          active={queuedAction?.kind === 'consulate'}
          disabled={isAnarchy || (a ? !a.can_build_consulate : false)}
          onClick={() => { queueAction({ kind: 'consulate' }); clearAllPickers(); }}
        />
        <ActionBtn
          label="🏰 Embassy"
          active={queuedAction?.kind === 'embassy'}
          disabled={isAnarchy || (a ? !a.can_build_embassy : false)}
          onClick={() => { queueAction({ kind: 'embassy' }); clearAllPickers(); }}
        />
        <ActionBtn
          label="🤝 Propose NAP"
          active={queuedAction?.kind === 'nap'}
          disabled={isAnarchy || (a ? !a.can_propose_nap : false)}
          onClick={() => { queueAction({ kind: 'nap' }); clearAllPickers(); }}
        />
        <ActionBtn
          label="🛡️ Propose Alliance"
          active={queuedAction?.kind === 'alliance'}
          disabled={isAnarchy || (a ? !a.can_propose_alliance : false)}
          onClick={() => { queueAction({ kind: 'alliance' }); clearAllPickers(); }}
        />
        <ActionBtn
          label="🕊️ Propose Peace"
          active={queuedAction?.kind === 'peace'}
          disabled={isAnarchy || (a ? !a.can_propose_peace : false)}
          onClick={() => { queueAction({ kind: 'peace' }); clearAllPickers(); }}
        />

        {/* Grant */}
        <div style={{ position: 'relative' }}>
          <ActionBtn
            label="💰 Send Grant"
            active={queuedAction?.kind === 'grant' || showGrantPicker}
            disabled={isAnarchy || (a ? !a.can_send_grant : false)}
            onClick={() => { setShowGrantPicker(p => !p); setShowBreakPicker(false); setConfirmWar(false); }}
          />
          {showGrantPicker && (
            <div style={{
              position: 'absolute', bottom: '100%', left: 0, marginBottom: 4,
              background: '#161625', border: '1px solid #3a3520',
              borderRadius: 4, padding: 6, display: 'flex', gap: 4, zIndex: 20,
            }}>
              {[500, 1000, 2000, 5000].map(amt => (
                <button key={amt} onClick={() => { queueAction({ kind: 'grant', amount: amt }); setShowGrantPicker(false); }}
                  style={pickerBtnStyle('#456')}>
                  ${amt}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Break Treaty */}
        <div style={{ position: 'relative' }}>
          <ActionBtn
            label="📜 Break Treaty"
            active={queuedAction?.kind === 'breakTreaty' || showBreakPicker}
            disabled={isAnarchy || (a ? !a.can_break_treaty : false)}
            onClick={() => { setShowBreakPicker(p => !p); setShowGrantPicker(false); setConfirmWar(false); }}
          />
          {showBreakPicker && a && a.breakable_treaties.length > 0 && (
            <div style={{
              position: 'absolute', bottom: '100%', left: 0, marginBottom: 4,
              background: '#161625', border: '1px solid #3a3520',
              borderRadius: 4, padding: 6, display: 'flex', gap: 4, zIndex: 20,
            }}>
              {a.breakable_treaties.map(t => (
                <button key={t} onClick={() => { queueAction({ kind: 'breakTreaty', treatyType: t }); setShowBreakPicker(false); }}
                  style={pickerBtnStyle('#a63')}>
                  {t}
                </button>
              ))}
            </div>
          )}
        </div>

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
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 12, color: '#e44' }}>Breaks all treaties. Sure?</span>
            <ActionBtn label="Confirm ⚔️" color="#a33"
              onClick={() => { queueAction({ kind: 'war' }); setConfirmWar(false); }} />
            <ActionBtn label="Cancel" color="#555"
              onClick={() => setConfirmWar(false)} />
          </div>
        )}
      </div>
    </div>
  );
}

function pickerBtnStyle(bg: string): CSSProperties {
  return {
    background: bg, color: '#e0d8c0', border: 'none',
    borderRadius: 3, padding: '4px 10px', fontSize: 12,
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
        color: '#e0d8c0',
        border: active ? '2px solid #daa520' : '1px solid #5a5030',
        borderRadius: 4,
        padding: active ? '6px 13px' : '7px 14px', // keep size consistent across border swap
        fontSize: 13,
        fontWeight: 'bold',
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.4 : 1,
        fontFamily: 'Georgia, serif',
        whiteSpace: 'nowrap',
        boxShadow: active ? '0 0 8px rgba(218,165,32,0.5)' : 'none',
        minHeight: 36,
      }}
    >
      {label}
    </button>
  );
}
