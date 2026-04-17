import { useState } from 'react';
import type { DiplomacyScreenData, DiplomacyScreenRelation } from '../wasm';

interface Props {
  diplomacy: DiplomacyScreenData;
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
  'Alliance': '#4a4',
  'NAP': '#4aa',
  'Neutral': '#888',
};

export default function DiplomacyPanel({
  diplomacy, onBuildConsulate, onBuildEmbassy, onProposeNap,
  onProposeAlliance, onDeclareWar, onSendGrant, onBreakTreaty, onProposePeace,
}: Props) {
  const [expandedNation, setExpandedNation] = useState<number | null>(null);
  const [confirmWar, setConfirmWar] = useState<number | null>(null);

  // Sort: at war first, then by score descending
  const sorted = [...diplomacy.relations].sort((a, b) => {
    if (a.at_war !== b.at_war) return a.at_war ? -1 : 1;
    return b.score - a.score;
  });

  return (
    <div style={{ fontSize: 13 }}>
      {/* Standing */}
      <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Standing</div>
      <div style={{ marginBottom: 8 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <div style={{
            flex: 1, height: 6, background: 'rgba(255,255,255,0.1)', borderRadius: 3, overflow: 'hidden',
          }}>
            <div style={{
              width: `${diplomacy.player_standing}%`, height: '100%',
              background: diplomacy.player_standing > 60 ? '#4a4' : diplomacy.player_standing > 30 ? '#ca4' : '#e44',
              borderRadius: 3,
            }} />
          </div>
          <span style={{ fontSize: 11, color: '#daa520', minWidth: 24 }}>{diplomacy.player_standing}</span>
        </div>
      </div>

      {/* Nations */}
      <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Nations</div>
      <div style={{ maxHeight: 'calc(100vh - 340px)', overflowY: 'auto' }}>
        {sorted.map(rel => (
          <NationRow
            key={rel.nation_id}
            rel={rel}
            isExpanded={expandedNation === rel.nation_id}
            confirmWar={confirmWar === rel.nation_id}
            onToggle={() => setExpandedNation(expandedNation === rel.nation_id ? null : rel.nation_id)}
            onBuildConsulate={() => onBuildConsulate(rel.nation_id)}
            onBuildEmbassy={() => onBuildEmbassy(rel.nation_id)}
            onProposeNap={() => onProposeNap(rel.nation_id)}
            onProposeAlliance={() => onProposeAlliance(rel.nation_id)}
            onDeclareWar={() => {
              if (confirmWar === rel.nation_id) {
                onDeclareWar(rel.nation_id);
                setConfirmWar(null);
              } else {
                setConfirmWar(rel.nation_id);
              }
            }}
            onCancelWar={() => setConfirmWar(null)}
            onSendGrant={(amt) => onSendGrant(rel.nation_id, amt)}
            onBreakTreaty={(tt) => onBreakTreaty(rel.nation_id, tt)}
            onProposePeace={() => onProposePeace(rel.nation_id)}
          />
        ))}
      </div>
    </div>
  );
}

interface NationRowProps {
  rel: DiplomacyScreenRelation;
  isExpanded: boolean;
  confirmWar: boolean;
  onToggle: () => void;
  onBuildConsulate: () => void;
  onBuildEmbassy: () => void;
  onProposeNap: () => void;
  onProposeAlliance: () => void;
  onDeclareWar: () => void;
  onCancelWar: () => void;
  onSendGrant: (amount: number) => void;
  onBreakTreaty: (treatyType: string) => void;
  onProposePeace: () => void;
}

function NationRow({
  rel, isExpanded, confirmWar, onToggle,
  onBuildConsulate, onBuildEmbassy, onProposeNap, onProposeAlliance,
  onDeclareWar, onCancelWar, onSendGrant, onBreakTreaty, onProposePeace,
}: NationRowProps) {
  const { actions } = rel;
  const scoreColor = rel.score > 30 ? '#4a4' : rel.score > -30 ? '#aaa' : '#e44';
  const scorePct = Math.max(0, Math.min(100, (rel.score + 100) / 2));

  return (
    <div style={{
      background: 'rgba(255,255,255,0.03)', borderRadius: 3,
      padding: '4px 5px', marginBottom: 3,
      borderLeft: `3px solid ${STATUS_COLORS[rel.status] || '#555'}`,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', cursor: 'pointer' }} onClick={onToggle}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ fontSize: 12, fontWeight: 'bold' }}>{rel.nation_name}</span>
          <span style={{ fontSize: 9, color: '#888', background: 'rgba(255,255,255,0.06)', borderRadius: 2, padding: '0 3px' }}>
            {rel.nation_type === 'GreatPower' ? 'GP' : 'MN'}
          </span>
        </div>
        <span style={{ fontSize: 11, color: STATUS_COLORS[rel.status] || '#888' }}>{rel.status}</span>
      </div>

      {/* Score bar */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 2 }}>
        <div style={{ flex: 1, height: 3, background: 'rgba(255,255,255,0.08)', borderRadius: 2, overflow: 'hidden' }}>
          <div style={{ width: `${scorePct}%`, height: '100%', background: scoreColor, borderRadius: 2 }} />
        </div>
        <span style={{ fontSize: 9, color: scoreColor, minWidth: 20 }}>{rel.score}</span>
      </div>

      {/* Treaty badges + pending proposal indicators */}
      {(rel.treaties.length > 0 || rel.has_pending_nap || rel.has_pending_alliance || rel.has_pending_peace) && (
        <div style={{ display: 'flex', gap: 3, marginTop: 3, flexWrap: 'wrap' }}>
          {rel.treaties.map(t => (
            <span key={t} style={{
              fontSize: 9, background: 'rgba(218,165,32,0.2)', color: '#daa520',
              borderRadius: 2, padding: '0 3px',
            }}>{t}</span>
          ))}
          {rel.has_pending_nap && (
            <span style={{ fontSize: 9, background: 'rgba(74,170,170,0.15)', color: '#4aa', borderRadius: 2, padding: '0 3px', fontStyle: 'italic' }}>NAP Proposed</span>
          )}
          {rel.has_pending_alliance && (
            <span style={{ fontSize: 9, background: 'rgba(74,170,74,0.15)', color: '#4a4', borderRadius: 2, padding: '0 3px', fontStyle: 'italic' }}>Alliance Proposed</span>
          )}
          {rel.has_pending_peace && (
            <span style={{ fontSize: 9, background: 'rgba(170,170,74,0.15)', color: '#aa4', borderRadius: 2, padding: '0 3px', fontStyle: 'italic' }}>Peace Proposed</span>
          )}
        </div>
      )}

      {/* Infrastructure */}
      <div style={{ display: 'flex', gap: 6, marginTop: 2, fontSize: 10, color: '#888' }}>
        {rel.has_consulate && <span>Consulate</span>}
        {rel.has_embassy && <span>Embassy</span>}
      </div>

      {/* Expanded action panel */}
      {isExpanded && (
        <div style={{ marginTop: 6, paddingTop: 4, borderTop: '1px solid #3a3520' }}>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 3 }}>
            {actions.can_build_consulate && (
              <ActionBtn label={`Consulate $${actions.consulate_cost}`} onClick={onBuildConsulate} />
            )}
            {actions.can_build_embassy && (
              <ActionBtn label={`Embassy $${actions.embassy_cost}`} onClick={onBuildEmbassy} />
            )}
            {actions.can_propose_nap && <ActionBtn label="Propose NAP" onClick={onProposeNap} />}
            {actions.can_propose_alliance && <ActionBtn label="Propose Alliance" onClick={onProposeAlliance} />}
            {actions.can_propose_peace && <ActionBtn label="Propose Peace" onClick={onProposePeace} color="#4a4" />}
            {actions.can_send_grant && (
              <div style={{ display: 'flex', gap: 2 }}>
                {[500, 1000, 2000, 5000].map(amt => (
                  <ActionBtn key={amt} label={`$${amt}`} onClick={() => onSendGrant(amt)} color="#456" />
                ))}
              </div>
            )}
            {actions.can_break_treaty && actions.breakable_treaties.map(t => (
              <ActionBtn key={t} label={`Break ${t}`} onClick={() => onBreakTreaty(t)} color="#a63" />
            ))}
            {actions.can_declare_war && !confirmWar && (
              <ActionBtn label="Declare War" onClick={onDeclareWar} color="#a33" />
            )}
            {confirmWar && (
              <div style={{ width: '100%', fontSize: 11, color: '#e44', marginTop: 2 }}>
                Breaks all treaties. Sure?
                <ActionBtn label="Confirm" onClick={onDeclareWar} color="#a33" />
                <ActionBtn label="Cancel" onClick={onCancelWar} color="#555" />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ActionBtn({ label, onClick, color }: { label: string; onClick: () => void; color?: string }) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onClick(); }}
      style={{
        background: color || '#3a3520', color: '#e0d8c0', border: 'none',
        borderRadius: 2, padding: '2px 5px', fontSize: 10, cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );
}
