import React, { useState } from 'react';
import type { GPLedgerEntry } from '../wasm';
import { resourceEmoji, resourceLabel } from '../resourceEmoji';

type Tab = 'economy' | 'cashflow' | 'production' | 'resources' | 'materials' | 'military' | 'diplomacy' | 'technology';

const NATION_COLORS: Record<string, string> = {
  Yellow: '#e6c619', Orange: '#e68a19', LightBlue: '#66b3ff', Red: '#e63946',
  Green: '#4caf50', Purple: '#9c27b0', Blue: '#3359e6',
};

interface Props {
  entries: GPLedgerEntry[];
  // Previous-turn snapshot used to render turn-over-turn deltas on every
  // numeric cell. `null` when no prior turn has been recorded yet (first
  // turn of a game).
  previousEntries: GPLedgerEntry[] | null;
  onClose: () => void;
}

// Build a lookup from the previous-turn snapshot, used by every table to
// compute per-cell deltas. Returns `null` if no snapshot is available.
function buildPrevMap(prev: GPLedgerEntry[] | null): Map<number, GPLedgerEntry> | null {
  if (!prev || prev.length === 0) return null;
  const m = new Map<number, GPLedgerEntry>();
  for (const e of prev) m.set(e.nation_id, e);
  return m;
}

export default function LedgerPanel({ entries, previousEntries, onClose }: Props) {
  const [tab, setTab] = useState<Tab>('economy');
  const [expanded, setExpanded] = useState<number | null>(null);
  const prevMap = buildPrevMap(previousEntries);

  const tabs: { key: Tab; label: string }[] = [
    { key: 'economy', label: 'Economy' },
    { key: 'cashflow', label: 'Cash flow' },
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
          {tab === 'economy' && <EconomyTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'cashflow' && <CashFlowTable entries={sorted} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'production' && <ProductionTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'resources' && <ResourcesTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'materials' && <MaterialsTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'military' && <MilitaryTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'diplomacy' && <DiplomacyTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
          {tab === 'technology' && <TechnologyTable entries={sorted} prevMap={prevMap} expanded={expanded} onExpand={setExpanded} />}
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

function EconomyTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
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
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          return (
            <React.Fragment key={e.nation_id}>
              <tr
                style={e.is_human ? styles.rowHuman : styles.row}
                onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
              >
                <td style={styles.tdName}><NationCell entry={e} /></td>
                <DeltaTd value={e.economy.treasury} prev={p?.economy.treasury} format={fmtMoneyCell} />
                <DeltaTd value={e.economy.provinces} prev={p?.economy.provinces} />
                <DeltaTd value={e.economy.goods_revenue} prev={p?.economy.goods_revenue} format={fmtMoneyCell} />
                <DeltaTd value={e.economy.total_resources} prev={p?.economy.total_resources} />
                <DeltaTd value={e.economy.total_materials} prev={p?.economy.total_materials} />
                <DeltaTd value={e.economy.total_goods} prev={p?.economy.total_goods} />
                <DeltaTd value={e.labor.total} prev={p?.labor.total} />
              </tr>
              {expanded === e.nation_id && (
                <tr style={styles.expandedRow}>
                  <td colSpan={8} style={styles.expandedCell}>
                    <div style={styles.detailGrid}>
                      <DetailItem label="Untrained" value={String(e.labor.untrained)} />
                      <DetailItem label="Trained" value={String(e.labor.trained)} />
                      <DetailItem label="Expert" value={String(e.labor.expert)} />
                    </div>
                    <CashCategoryBreakdown entry={e} />
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

function ProductionTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
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
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          return (
            <tr
              key={e.nation_id}
              style={e.is_human ? styles.rowHuman : styles.row}
              onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
            >
              <td style={styles.tdName}><NationCell entry={e} /></td>
              <DeltaTd value={e.economy.buildings} prev={p?.economy.buildings} />
              <DeltaTd value={e.labor.total} prev={p?.labor.total} />
              <DeltaTd value={e.labor.untrained} prev={p?.labor.untrained} />
              <DeltaTd value={e.labor.trained} prev={p?.labor.trained} />
              <DeltaTd value={e.labor.expert} prev={p?.labor.expert} />
              <DeltaTd value={e.economy.goods_revenue} prev={p?.economy.goods_revenue} format={fmtMoneyCell} />
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

const RESOURCE_ORDER = ['Timber', 'Coal', 'Iron', 'Cotton', 'Wool', 'Grain', 'Fruit', 'Livestock', 'Horses', 'Oil', 'Gold', 'Gems'];

function ResourcesTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          {RESOURCE_ORDER.map(r => <Th key={r} text={`${resourceEmoji(r)} ${r}`} />)}
          <Th text="Total" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          return (
            <React.Fragment key={e.nation_id}>
              <tr
                style={e.is_human ? styles.rowHuman : styles.row}
                onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
              >
                <td style={styles.tdName}><NationCell entry={e} /></td>
                {RESOURCE_ORDER.map(r => {
                  const cur = e.resources_detail?.[r] || 0;
                  const prv = p ? (p.resources_detail?.[r] || 0) : undefined;
                  return <DeltaTd key={r} value={cur} prev={prv} highlight={cur > 0} />;
                })}
                <DeltaTd value={e.economy.total_resources} prev={p?.economy.total_resources} highlight />
              </tr>
              {expanded === e.nation_id && (
                <tr style={styles.expandedRow}>
                  <td colSpan={RESOURCE_ORDER.length + 2} style={styles.expandedCell}>
                    <StockpileCategoryBreakdown entry={e} stockpiles={RESOURCE_ORDER} />
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

const MATERIAL_ORDER = ['Lumber', 'Steel', 'Fabric', 'Paper', 'Arms', 'CannedFood'];
const MATERIAL_LABELS: Record<string, string> = { CannedFood: 'Canned Food' };
const GOODS_ORDER = ['Furniture', 'Clothing', 'Hardware'];

function MaterialsTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
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
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          const colCount = MATERIAL_ORDER.length + GOODS_ORDER.length + 3;
          return (
            <React.Fragment key={e.nation_id}>
              <tr
                style={e.is_human ? styles.rowHuman : styles.row}
                onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
              >
                <td style={styles.tdName}><NationCell entry={e} /></td>
                {MATERIAL_ORDER.map(m => {
                  const cur = e.materials_detail?.[m] || 0;
                  const prv = p ? (p.materials_detail?.[m] || 0) : undefined;
                  return <DeltaTd key={m} value={cur} prev={prv} highlight={cur > 0} />;
                })}
                <DeltaTd value={e.economy.total_materials} prev={p?.economy.total_materials} highlight />
                {GOODS_ORDER.map(g => {
                  const cur = e.goods_detail?.[g] || 0;
                  const prv = p ? (p.goods_detail?.[g] || 0) : undefined;
                  return <DeltaTd key={g} value={cur} prev={prv} highlight={cur > 0} highlightColor="#2a9d8f" />;
                })}
                <DeltaTd value={e.economy.total_goods} prev={p?.economy.total_goods} highlight highlightColor="#2a9d8f" />
              </tr>
              {expanded === e.nation_id && (
                <tr style={styles.expandedRow}>
                  <td colSpan={colCount} style={styles.expandedCell}>
                    <StockpileCategoryBreakdown entry={e} stockpiles={[...MATERIAL_ORDER, ...GOODS_ORDER]} />
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

function MilitaryTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
  const maxFp = Math.max(...entries.map(x => x.military.total_army_fp));
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Field Army" />
          <Th text="Militia" />
          <Th text="Firepower" />
          <Th text="Warships" />
          <Th text="Merchants" />
          <Th text="Arms Built" />
          <Th text="Generals" />
        </tr>
      </thead>
      <tbody>
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          return (
            <tr
              key={e.nation_id}
              style={e.is_human ? styles.rowHuman : styles.row}
              onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
            >
              <td style={styles.tdName}><NationCell entry={e} /></td>
              <DeltaTd value={e.military.field_army_count} prev={p?.military.field_army_count} />
              <DeltaTd value={e.military.militia_count} prev={p?.military.militia_count} />
              <DeltaTd value={e.military.total_army_fp} prev={p?.military.total_army_fp} highlight={e.military.total_army_fp === maxFp} />
              <DeltaTd value={e.military.total_warship_count} prev={p?.military.total_warship_count} />
              <DeltaTd value={e.military.merchant_ships} prev={p?.military.merchant_ships} />
              <DeltaTd value={e.military.total_arms_built} prev={p?.military.total_arms_built} />
              <DeltaTd value={e.military.generals_earned} prev={p?.military.generals_earned} />
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function DiplomacyTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
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
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          return (
          <React.Fragment key={e.nation_id}>
            <tr
              style={e.is_human ? styles.rowHuman : styles.row}
              onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
            >
              <td style={styles.tdName}><NationCell entry={e} /></td>
              <DeltaTd value={e.diplomacy.standing} prev={p?.diplomacy.standing} />
              <DeltaTd value={e.diplomacy.consulates} prev={p?.diplomacy.consulates} />
              <DeltaTd value={e.diplomacy.embassies} prev={p?.diplomacy.embassies} />
              <DeltaTd value={e.diplomacy.alliances} prev={p?.diplomacy.alliances} highlight={e.diplomacy.alliances > 0} highlightColor="#2ecc40" />
              <DeltaTd value={e.diplomacy.wars} prev={p?.diplomacy.wars} highlight={e.diplomacy.wars > 0} highlightColor="#e63946" />
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
          );
        })}
      </tbody>
    </table>
  );
}

function TechnologyTable({ entries, prevMap, expanded, onExpand }: { entries: GPLedgerEntry[]; prevMap: Map<number, GPLedgerEntry> | null; expanded: number | null; onExpand: (id: number | null) => void }) {
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
        {entries.map(e => {
          const p = prevMap?.get(e.nation_id);
          const curList = e.technology?.researched_names || [];
          const prvList = p?.technology?.researched_names || [];
          const newTechs = prvList.length > 0 ? curList.filter(t => !prvList.includes(t)) : [];
          return (
            <React.Fragment key={e.nation_id}>
              <tr
                style={e.is_human ? styles.rowHuman : styles.row}
                onClick={() => onExpand(expanded === e.nation_id ? null : e.nation_id)}
              >
                <td style={styles.tdName}><NationCell entry={e} /></td>
                <DeltaTd value={e.technology?.researched_count || 0} prev={p?.technology?.researched_count} />
                <td style={{ ...styles.td, textAlign: 'left', color: '#999', fontSize: 12 }}>
                  {curList.length === 0 ? 'None' : curList.map((t, i) => (
                    <span key={t} style={{ color: newTechs.includes(t) ? '#2ecc40' : '#999' }}>
                      {i > 0 && <span style={{ color: '#555' }}>, </span>}
                      {t}
                    </span>
                  ))}
                </td>
              </tr>
            </React.Fragment>
          );
        })}
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

// DeltaTd renders a numeric cell plus a small colored delta chip showing
// the turn-over-turn change. When `prev` is undefined (no prior snapshot,
// or new nation this turn) the chip is omitted.
function DeltaTd({
  value,
  prev,
  format,
  highlight,
  highlightColor,
}: {
  value: number;
  prev: number | undefined;
  format?: (n: number) => string;
  highlight?: boolean;
  highlightColor?: string;
}) {
  const fmt = format ?? ((n: number) => n.toLocaleString());
  const showDelta = prev !== undefined && prev !== value;
  const delta = showDelta ? value - (prev as number) : 0;
  const deltaStr = showDelta
    ? (delta > 0 ? `+${fmt(delta)}` : `-${fmt(-delta)}`)
    : '';
  const deltaColor = delta > 0 ? '#2ecc40' : delta < 0 ? '#e63946' : '#777';
  return (
    <td
      style={{
        ...styles.td,
        color: highlight ? (highlightColor || '#daa520') : '#bbb',
        fontWeight: highlight ? 'bold' : 'normal',
      }}
    >
      <span>{fmt(value)}</span>
      {showDelta && (
        <span
          style={{
            display: 'inline-block',
            marginLeft: 4,
            fontSize: 10,
            color: deltaColor,
            fontWeight: 'normal',
          }}
        >
          {deltaStr}
        </span>
      )}
    </td>
  );
}

const fmtMoneyCell = (n: number) => `$${n.toLocaleString()}`;

const CATEGORY_COLORS: Record<string, string> = {
  Production: '#2ecc40',  // green: what you made
  Trade: '#66b3ff',       // blue: market
  Consumption: '#e67e22', // orange: used up
};

const CATEGORY_ORDER = ['Production', 'Trade', 'Consumption'];

/// Roll-up of this turn's cash flow bucketed into Production / Trade /
/// Consumption, shown inside the Economy tab's expanded row.
function CashCategoryBreakdown({ entry }: { entry: GPLedgerEntry }) {
  const cf = entry.cash_flow;
  if (!cf) return null;
  const hasIncome = Object.values(cf.income_by_category || {}).some(v => v > 0);
  const hasExpense = Object.values(cf.expense_by_category || {}).some(v => v > 0);
  if (!hasIncome && !hasExpense) return null;
  return (
    <div style={{ marginTop: 10, borderTop: '1px solid #2a2a3a', paddingTop: 8 }}>
      <div style={{ fontSize: 11, color: '#888', marginBottom: 4, textTransform: 'uppercase', letterSpacing: 0.5 }}>
        This turn's cash flow by category
      </div>
      <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', fontSize: 12, fontFamily: 'monospace' }}>
        <div>
          <span style={{ color: '#2ecc40', fontWeight: 'bold', marginRight: 6 }}>Income:</span>
          {CATEGORY_ORDER.map(c => {
            const v = cf.income_by_category?.[c] || 0;
            if (v === 0) return null;
            return (
              <span key={c} style={{ marginRight: 12, color: CATEGORY_COLORS[c] || '#ccc' }}>
                {c} +${v.toLocaleString()}
              </span>
            );
          })}
          {!hasIncome && <span style={{ color: '#555' }}>(none)</span>}
        </div>
        <div>
          <span style={{ color: '#e63946', fontWeight: 'bold', marginRight: 6 }}>Expense:</span>
          {CATEGORY_ORDER.map(c => {
            const v = cf.expense_by_category?.[c] || 0;
            if (v === 0) return null;
            return (
              <span key={c} style={{ marginRight: 12, color: CATEGORY_COLORS[c] || '#ccc' }}>
                {c} −${v.toLocaleString()}
              </span>
            );
          })}
          {!hasExpense && <span style={{ color: '#555' }}>(none)</span>}
        </div>
      </div>
    </div>
  );
}

/// Per-stockpile breakdown of this turn's resource / material / goods
/// inflow and outflow, grouped into Production / Trade / Consumption.
/// Hidden when the nation had no movement for any of the given stockpiles
/// (e.g. fresh game on turn 1).
function StockpileCategoryBreakdown({ entry, stockpiles }: { entry: GPLedgerEntry; stockpiles: string[] }) {
  const rf = entry.resource_flow;
  if (!rf) return null;
  const inMap = rf.inflow_by_stockpile_category || {};
  const outMap = rf.outflow_by_stockpile_category || {};
  const rows = stockpiles
    .map(stock => ({
      stock,
      inCat: inMap[stock] || {},
      outCat: outMap[stock] || {},
    }))
    .filter(r => Object.keys(r.inCat).length > 0 || Object.keys(r.outCat).length > 0);
  if (rows.length === 0) {
    return (
      <div style={{ fontSize: 12, color: '#666', padding: 4 }}>
        No in/out movement this turn for these stockpiles.
      </div>
    );
  }
  return (
    <div>
      <div style={{ fontSize: 11, color: '#888', marginBottom: 4, textTransform: 'uppercase', letterSpacing: 0.5 }}>
        This turn's flow by category (production / trade / consumption)
      </div>
      <table style={{ ...styles.table, fontSize: 12 }}>
        <thead>
          <tr>
            <th style={{ ...styles.th, textAlign: 'left' }}>Stockpile</th>
            <th style={{ ...styles.th, color: '#2ecc40' }}>+ Production</th>
            <th style={{ ...styles.th, color: '#66b3ff' }}>+ Trade</th>
            <th style={{ ...styles.th, color: '#e63946' }}>− Consumption</th>
            <th style={{ ...styles.th, color: '#66b3ff' }}>− Trade</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(r => (
            <tr key={r.stock}>
              <td style={{ ...styles.tdName, color: '#daa520' }}>{resourceLabel(r.stock)}</td>
              <td style={styles.td}>{fmtCatAmount(r.inCat.Production)}</td>
              <td style={styles.td}>{fmtCatAmount(r.inCat.Trade)}</td>
              <td style={styles.td}>{fmtCatAmount(r.outCat.Consumption)}</td>
              <td style={styles.td}>{fmtCatAmount(r.outCat.Trade)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function fmtCatAmount(n: number | undefined): string {
  if (!n) return '·';
  return n.toLocaleString();
}

function DetailItem({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div style={{ fontSize: 12 }}>
      <span style={{ color: '#777' }}>{label}: </span>
      <span style={{ color: color || '#ccc' }}>{value}</span>
    </div>
  );
}

function fmtMoney(n: number): string {
  const sign = n < 0 ? '-' : '';
  return `${sign}$${Math.abs(n).toLocaleString()}`;
}

function CashFlowTable({ entries, expanded, onExpand }: { entries: GPLedgerEntry[]; expanded: number | null; onExpand: (id: number | null) => void }) {
  return (
    <table style={styles.table}>
      <thead>
        <tr>
          <Th text="Nation" align="left" />
          <Th text="Opening" />
          <Th text="Closing" />
          <Th text="Δ" />
          <Th text="Income" />
          <Th text="Expense" />
          <Th text="Reconcile" />
        </tr>
      </thead>
      <tbody>
        {entries.map((e) => {
          const cf = e.cash_flow;
          const isOpen = expanded === e.nation_id;
          return (
            <React.Fragment key={e.nation_id}>
              <tr
                style={e.is_human ? styles.rowHuman : styles.row}
                onClick={() => onExpand(isOpen ? null : e.nation_id)}
              >
                <td style={styles.tdName}><NationCell entry={e} /></td>
                <td style={styles.td}>{cf ? fmtMoney(cf.opening_treasury) : '—'}</td>
                <td style={styles.td}>{cf ? fmtMoney(cf.closing_treasury) : '—'}</td>
                <td style={{
                  ...styles.td,
                  color: cf ? (cf.observed_delta >= 0 ? '#2ecc40' : '#e63946') : '#888',
                  fontWeight: 'bold',
                }}>{cf ? fmtMoney(cf.observed_delta) : '—'}</td>
                <td style={{ ...styles.td, color: '#2ecc40' }}>{cf ? fmtMoney(cf.total_income) : '—'}</td>
                <td style={{ ...styles.td, color: '#e63946' }}>{cf ? fmtMoney(cf.total_expense) : '—'}</td>
                <td style={{
                  ...styles.td,
                  color: cf ? (cf.reconciles ? '#2ecc40' : '#e63946') : '#888',
                }}>
                  {cf ? (cf.reconciles ? 'OK' : `Δ ${fmtMoney(cf.reconciliation_mismatch)}`) : '—'}
                </td>
              </tr>
              {isOpen && cf && (
                <tr style={styles.expandedRow}>
                  <td colSpan={7} style={styles.expandedCell}>
                    <div style={styles.detailGrid}>
                      <div style={{ minWidth: 220 }}>
                        <div style={{ color: '#2ecc40', fontSize: 12, marginBottom: 4, fontWeight: 'bold' }}>
                          Income (${cf.total_income.toLocaleString()})
                        </div>
                        {Object.entries(cf.income_totals).length === 0
                          ? <div style={{ color: '#666', fontSize: 11 }}>(no income this turn)</div>
                          : Object.entries(cf.income_totals)
                              .sort((a, b) => b[1] - a[1])
                              .map(([label, amount]) => (
                                <DetailItem key={label} label={label} value={fmtMoney(amount)} color="#2ecc40" />
                              ))}
                      </div>
                      <div style={{ minWidth: 220 }}>
                        <div style={{ color: '#e63946', fontSize: 12, marginBottom: 4, fontWeight: 'bold' }}>
                          Expense (${cf.total_expense.toLocaleString()})
                        </div>
                        {Object.entries(cf.expense_totals).length === 0
                          ? <div style={{ color: '#666', fontSize: 11 }}>(no expense this turn)</div>
                          : Object.entries(cf.expense_totals)
                              .sort((a, b) => b[1] - a[1])
                              .map(([label, amount]) => (
                                <DetailItem key={label} label={label} value={fmtMoney(amount)} color="#e63946" />
                              ))}
                      </div>
                      <div style={{ minWidth: 220 }}>
                        <div style={{ color: '#daa520', fontSize: 12, marginBottom: 4, fontWeight: 'bold' }}>
                          Cumulative (all turns)
                        </div>
                        {Object.entries(e.cumulative.income_totals).map(([k, v]) => (
                          <DetailItem key={`c-in-${k}`} label={`+ ${k}`} value={fmtMoney(v)} color="#2ecc40" />
                        ))}
                        {Object.entries(e.cumulative.expense_totals).map(([k, v]) => (
                          <DetailItem key={`c-out-${k}`} label={`− ${k}`} value={fmtMoney(v)} color="#e63946" />
                        ))}
                      </div>
                    </div>
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
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
