import React, { useState } from 'react';
import type { GPLedgerEntry } from '../wasm';

type Tab = 'economy' | 'production' | 'resources' | 'materials' | 'military' | 'diplomacy' | 'technology';

const NATION_COLORS: Record<string, string> = {
  Yellow: '#e6c619', Orange: '#e68a19', LightBlue: '#66b3ff', Red: '#e63946',
  Green: '#4caf50', Purple: '#9c27b0', Blue: '#3359e6',
};

interface Props {
  entries: GPLedgerEntry[];
  onClose: () => void;
}

export default function LedgerPanel({ entries, onClose }: Props) {
  const [tab, setTab] = useState<Tab>('economy');
  const [expanded, setExpanded] = useState<number | null>(null);

  const tabs: { key: Tab; label: string }[] = [
    { key: 'economy', label: 'Economy' },
    { key: 'production', label: 'Production' },
    { key: 'resources', label: 'Resources' },
    { key: 'materials', label: 'Materials' },
    { key: 'military', label: 'Military' },
    { key: 'diplomacy', label: 'Diplomacy' },
    { key: 'technology', label: 'Technology' },
  ];

  const sorted = [...entries].sort((a, b) => {
    if (a.is_human && !b.is_human) return -1;
    if (!a.is_human && b.is_human) return 1;
    return b.economy.treasury - a.economy.treasury;
  });

  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        <div style={styles.header}>
          <h2 style={styles.title}>National Ledger</h2>
          <div style={styles.tabBar}>
            {tabs.map(t => (
              <button
                key={t.key}
                onClick={() => setTab(t.key)}
                style={tab === t.key ? { ...styles.tab, ...styles.tabActive } : styles.tab}
              >
                {t.label}
              </button>
            ))}
          </div>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>

        <div style={styles.tableWrap}>
          {tab === 'economy' && <EconomyTable entries={sorted} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'production' && <ProductionTable entries={sorted} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'resources' && <ResourcesTable entries={sorted} />}
          {tab === 'materials' && <MaterialsTable entries={sorted} />}
          {tab === 'military' && <MilitaryTable entries={sorted} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'diplomacy' && <DiplomacyTable entries={sorted} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'technology' && <TechnologyTable entries={sorted} expanded={expanded} onExpand={setExpanded} />}
        </div>
      </div>
    </div>
  );
}

function NationCell({ entry }: { entry: GPLedgerEntry }) {
  const color = NATION_COLORS[entry.nation_color] || '#aaa';
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
      <span style={{ width: 10, height: 10, borderRadius: '50%', background: color, display: 'inline-block', flexShrink: 0 }} />
      <span style={{ color: entry.is_human ? '#daa520' : '#ccc', fontWeight: entry.is_human ? 'bold' : 'normal' }}>
        {entry.nation_name}
      </span>
    </div>
  );
}

function EconomyTable({ entries, expanded, onExpand }: { entries: GPLedgerEntry[]; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Treasury" />
          <Th text="Provinces" />
          <Th text="Revenue" />
          <Th text="Total Resources" />
          <Th text="Total Materials" />
          <Th text="Total Goods" />
          <Th text="Workers" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <React.Fragment key={e.nation_id}>
            <tr
              style={e.is_human ? styles.rowHuman : styles.row}
              onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
            >
              <td style={styles.tdName}><NationCell entry={e} /></td>
              <Td value={`$${e.economy.treasury.toLocaleString()}`} />
              <Td value={String(e.economy.provinces)} />
              <Td value={`$${e.economy.goods_revenue.toLocaleString()}`} />
              <Td value={String(e.economy.total_resources)} />
              <Td value={String(e.economy.total_materials)} />
              <Td value={String(e.economy.total_goods)} />
              <Td value={String(e.labor.total)} />
            </tr>
            {expanded === e.nation_id && (
              <tr style={styles.expandedRow}>
                <td colSpan={8} style={styles.expandedCell}>
                  <div style={styles.detailGrid}>
                    <DetailItem label="Untrained" value={String(e.labor.untrained)} />
                    <DetailItem label="Trained" value={String(e.labor.trained)} />
                    <DetailItem label="Expert" value={String(e.labor.expert)} />
                  </div>
                </td>
              </tr>
            )}
          </React.Fragment>
        ))}
      </tbody>
    </table>
  );
}

function ProductionTable({ entries, expanded, onExpand }: { entries: GPLedgerEntry[]; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Buildings" />
          <Th text="Workers" />
          <Th text="Untrained" />
          <Th text="Trained" />
          <Th text="Expert" />
          <Th text="Revenue" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <tr
            key={e.nation_id}
            style={e.is_human ? styles.rowHuman : styles.row}
            onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
          >
            <td style={styles.tdName}><NationCell entry={e} /></td>
            <Td value={String(e.economy.buildings)} />
            <Td value={String(e.labor.total)} />
            <Td value={String(e.labor.untrained)} />
            <Td value={String(e.labor.trained)} />
            <Td value={String(e.labor.expert)} />
            <Td value={`$${e.economy.goods_revenue.toLocaleString()}`} />
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const RESOURCE_ORDER = ['Timber', 'Coal', 'Iron', 'Cotton', 'Wool', 'Grain', 'Fruit', 'Livestock', 'Horses', 'Oil', 'Gold', 'Gems'];

function ResourcesTable({ entries }: { entries: GPLedgerEntry[] }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          {RESOURCE_ORDER.map(r => <Th key={r} text={r} />)}
          <Th text="Total" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <tr key={e.nation_id} style={e.is_human ? styles.rowHuman : styles.row}>
            <td style={styles.tdName}><NationCell entry={e} /></td>
            {RESOURCE_ORDER.map(r => (
              <Td key={r} value={String(e.resources_detail?.[r] || 0)} highlight={(e.resources_detail?.[r] || 0) > 0} />
            ))}
            <Td value={String(e.economy.total_resources)} highlight />
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const MATERIAL_ORDER = ['Lumber', 'Steel', 'Fabric', 'Paper', 'Arms', 'CannedFood'];
const MATERIAL_LABELS: Record<string, string> = { CannedFood: 'Canned Food' };
const GOODS_ORDER = ['Furniture', 'Clothing', 'Hardware'];

function MaterialsTable({ entries }: { entries: GPLedgerEntry[] }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          {MATERIAL_ORDER.map(m => <Th key={m} text={MATERIAL_LABELS[m] || m} />)}
          <Th text="Mat. Total" />
          {GOODS_ORDER.map(g => <Th key={g} text={g} />)}
          <Th text="Goods Total" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <tr key={e.nation_id} style={e.is_human ? styles.rowHuman : styles.row}>
            <td style={styles.tdName}><NationCell entry={e} /></td>
            {MATERIAL_ORDER.map(m => (
              <Td key={m} value={String(e.materials_detail?.[m] || 0)} highlight={(e.materials_detail?.[m] || 0) > 0} />
            ))}
            <Td value={String(e.economy.total_materials)} highlight />
            {GOODS_ORDER.map(g => (
              <Td key={g} value={String(e.goods_detail?.[g] || 0)} highlight={(e.goods_detail?.[g] || 0) > 0} highlightColor="#2a9d8f" />
            ))}
            <Td value={String(e.economy.total_goods)} highlight highlightColor="#2a9d8f" />
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function MilitaryTable({ entries, expanded, onExpand }: { entries: GPLedgerEntry[]; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Army (field + militia)" />
          <Th text="Firepower" />
          <Th text="Warships" />
          <Th text="Merchants" />
          <Th text="Arms Built" />
          <Th text="Generals" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <tr
            key={e.nation_id}
            style={e.is_human ? styles.rowHuman : styles.row}
            onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
          >
            <td style={styles.tdName}><NationCell entry={e} /></td>
            <Td value={`${e.military.field_army_count} + ${e.military.militia_count}m`} />
            <Td value={String(e.military.total_army_fp)} highlight={e.military.total_army_fp === Math.max(...entries.map(x => x.military.total_army_fp))} />
            <Td value={String(e.military.total_warship_count)} />
            <Td value={String(e.military.merchant_ships)} />
            <Td value={String(e.military.total_arms_built)} />
            <Td value={String(e.military.generals_earned)} />
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function DiplomacyTable({ entries, expanded, onExpand }: { entries: GPLedgerEntry[]; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Standing" />
          <Th text="Consulates" />
          <Th text="Embassies" />
          <Th text="Alliances" />
          <Th text="Wars" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <React.Fragment key={e.nation_id}>
            <tr
              style={e.is_human ? styles.rowHuman : styles.row}
              onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
            >
              <td style={styles.tdName}><NationCell entry={e} /></td>
              <Td value={String(e.diplomacy.standing)} />
              <Td value={String(e.diplomacy.consulates)} />
              <Td value={String(e.diplomacy.embassies)} />
              <Td value={String(e.diplomacy.alliances)} highlight={e.diplomacy.alliances > 0} highlightColor="#2ecc40" />
              <Td value={String(e.diplomacy.wars)} highlight={e.diplomacy.wars > 0} highlightColor="#e63946" />
            </tr>
            {expanded === e.nation_id && (e.diplomacy.alliance_names.length > 0 || e.diplomacy.war_names.length > 0) && (
              <tr style={styles.expandedRow}>
                <td colSpan={6} style={styles.expandedCell}>
                  <div style={styles.detailGrid}>
                    {e.diplomacy.alliance_names.length > 0 && (
                      <DetailItem label="Allied with" value={e.diplomacy.alliance_names.join(', ')} color="#2ecc40" />
                    )}
                    {e.diplomacy.war_names.length > 0 && (
                      <DetailItem label="At war with" value={e.diplomacy.war_names.join(', ')} color="#e63946" />
                    )}
                  </div>
                </td>
              </tr>
            )}
          </React.Fragment>
        ))}
      </tbody>
    </table>
  );
}

function TechnologyTable({ entries, expanded, onExpand }: { entries: GPLedgerEntry[]; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Researched" />
          <Th text="Technologies" align="left" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => (
          <React.Fragment key={e.nation_id}>
            <tr
              style={e.is_human ? styles.rowHuman : styles.row}
              onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
            >
              <td style={styles.tdName}><NationCell entry={e} /></td>
              <Td value={String(e.technology?.researched_count || 0)} />
              <td style={{ ...styles.td, textAlign: 'left', color: '#999', fontSize: 12 }}>
                {(e.technology?.researched_names || []).join(', ') || 'None'}
              </td>
            </tr>
          </React.Fragment>
        ))}
      </tbody>
    </table>
  );
}

function Th({ text, align }: { text: string; align?: string }) {
  return (
    <th style={{ ...styles.th, textAlign: (align || 'right') as any }}>
      {text}
    </th>
  );
}

function Td({ value, highlight, highlightColor }: { value: string; highlight?: boolean; highlightColor?: string }) {
  return (
    <td style={{
      ...styles.td,
      color: highlight ? (highlightColor || '#daa520') : '#bbb',
      fontWeight: highlight ? 'bold' : 'normal',
    }}>
      {value}
    </td>
  );
}

function DetailItem({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div style={{ fontSize: 12 }}>
      <span style={{ color: '#777' }}>{label}: </span>
      <span style={{ color: color || '#ccc' }}>{value}</span>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    flex: 1, minHeight: 0, background: '#111118',
    display: 'flex', flexDirection: 'column',
  },
  container: {
    flex: 1, display: 'flex', flexDirection: 'column', padding: '16px 24px',
    maxWidth: 1200, margin: '0 auto', width: '100%',
  },
  header: {
    display: 'flex', alignItems: 'center', gap: 16, marginBottom: 16,
    borderBottom: '2px solid #333', paddingBottom: 12,
  },
  title: {
    margin: 0, fontSize: 22, fontFamily: 'Georgia, serif', color: '#daa520',
    whiteSpace: 'nowrap',
  },
  tabBar: { display: 'flex', gap: 4, flex: 1, flexWrap: 'wrap' },
  tab: {
    background: 'transparent', border: '1px solid #333', color: '#888',
    padding: '6px 12px', cursor: 'pointer', fontFamily: 'Georgia, serif',
    fontSize: 12, borderRadius: 4,
  },
  tabActive: {
    color: '#daa520', borderColor: '#daa520', background: 'rgba(218,165,32,0.08)',
  },
  closeBtn: {
    background: 'transparent', border: '1px solid #555', color: '#888',
    padding: '4px 12px', cursor: 'pointer', fontSize: 12, borderRadius: 4,
  },
  tableWrap: { flex: 1, overflowY: 'auto', overflowX: 'auto' },
  table: {
    width: '100%', borderCollapse: 'collapse' as const, fontSize: 14,
    fontFamily: "'Segoe UI', sans-serif",
  },
  th: {
    padding: '8px 12px', color: '#daa520', fontWeight: 'bold', fontSize: 11,
    textTransform: 'uppercase' as const, letterSpacing: 0.5,
    borderBottom: '1px solid #333', whiteSpace: 'nowrap' as const,
  },
  row: {
    cursor: 'pointer', borderBottom: '1px solid #1e1e30',
  },
  rowHuman: {
    cursor: 'pointer', borderBottom: '1px solid #1e1e30',
    background: 'rgba(218,165,32,0.04)',
  },
  tdName: { padding: '8px 12px', whiteSpace: 'nowrap' as const },
  td: {
    padding: '8px 12px', textAlign: 'right' as const,
    fontFamily: 'monospace', fontSize: 13,
  },
  expandedRow: { background: 'rgba(218,165,32,0.03)' },
  expandedCell: { padding: '6px 12px 10px 32px' },
  detailGrid: { display: 'flex', gap: 20, flexWrap: 'wrap' as const },
};
