import React, { useState } from 'react';
import type { LedgerData } from '../wasm';

type Tab = 'economy' | 'production' | 'military' | 'diplomacy';

interface Props {
  ledger: LedgerData;
}

export default function LedgerPanel({ ledger }: Props) {
  const [tab, setTab] = useState<Tab>('economy');

  const tabs: { key: Tab; label: string }[] = [
    { key: 'economy', label: 'Economy' },
    { key: 'production', label: 'Production' },
    { key: 'military', label: 'Military' },
    { key: 'diplomacy', label: 'Diplomacy' },
  ];

  return (
    <div style={styles.container}>
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

      <div style={styles.body}>
        {tab === 'economy' && <EconomyTab ledger={ledger} />}
        {tab === 'production' && <ProductionTab ledger={ledger} />}
        {tab === 'military' && <MilitaryTab ledger={ledger} />}
        {tab === 'diplomacy' && <DiplomacyTab ledger={ledger} />}
      </div>
    </div>
  );
}

function EconomyTab({ ledger }: { ledger: LedgerData }) {
  const { economy, labor } = ledger;
  return (
    <div>
      <SectionLabel text="Treasury" />
      <Row label="Balance" value={`$${economy.treasury.toLocaleString()}`} />
      <Row label="Goods Revenue (cumulative)" value={`$${economy.goods_revenue.toLocaleString()}`} />

      {economy.subsidies.length > 0 && (
        <>
          <SectionLabel text="Subsidies" />
          {economy.subsidies.map((s, i) => (
            <Row key={i} label={s.nation} value={`$${s.amount}/turn`} />
          ))}
        </>
      )}

      <SectionLabel text="Labor Force" />
      <Row label="Untrained Workers" value={String(labor.untrained)} />
      <Row label="Trained Workers" value={String(labor.trained)} />
      <Row label="Expert Workers" value={String(labor.expert)} />
      <Row label="Total" value={String(labor.total)} highlight />
    </div>
  );
}

function ProductionTab({ ledger }: { ledger: LedgerData }) {
  const { production } = ledger;
  return (
    <div>
      <SectionLabel text="Buildings" />
      {production.buildings.length === 0 && <EmptyNote text="No buildings" />}
      {production.buildings.map((b, i) => (
        <Row key={i} label={b.type.replace(/([A-Z])/g, ' $1').trim()} value={`Cap: ${b.capacity}${b.upgrading ? ' (upgrading)' : ''}`} />
      ))}

      <SectionLabel text="Resources" />
      {production.resources.length === 0 && <EmptyNote text="Warehouse empty" />}
      {production.resources.map((r, i) => (
        <Row key={i} label={r.name} value={String(r.quantity)} />
      ))}

      <SectionLabel text="Materials" />
      {production.materials.length === 0 && <EmptyNote text="No materials" />}
      {production.materials.map((m, i) => (
        <Row key={i} label={m.name} value={String(m.quantity)} />
      ))}

      <SectionLabel text="Goods" />
      {production.goods.length === 0 && <EmptyNote text="No goods" />}
      {production.goods.map((g, i) => (
        <Row key={i} label={g.name} value={String(g.quantity)} />
      ))}
    </div>
  );
}

function MilitaryTab({ ledger }: { ledger: LedgerData }) {
  const { military } = ledger;
  return (
    <div>
      <SectionLabel text="Army" />
      {military.army_by_type.length === 0 && <EmptyNote text="No army units" />}
      {military.army_by_type.map((u, i) => (
        <Row key={i} label={u.unit_type.replace(/([A-Z])/g, ' $1').trim()} value={`${u.count} (FP: ${u.firepower})`} />
      ))}
      <Row label="Total Army" value={`${military.total_army_count} units, ${military.total_army_fp} FP`} highlight />
      <Row label="Arms Built" value={String(military.total_arms_built)} />
      <Row label="Generals Earned" value={String(military.generals_earned)} />

      <SectionLabel text="Navy" />
      {military.warships_by_type.length === 0 && <EmptyNote text="No warships" />}
      {military.warships_by_type.map((s, i) => (
        <Row key={i} label={s.ship_type.replace(/([A-Z])/g, ' $1').trim()} value={String(s.count)} />
      ))}
      <Row label="Total Warships" value={String(military.total_warship_count)} highlight />
      <Row label="Merchant Ships" value={String(military.merchant_ships)} />
    </div>
  );
}

function DiplomacyTab({ ledger }: { ledger: LedgerData }) {
  const { diplomacy } = ledger;
  return (
    <div>
      <SectionLabel text="Standing" />
      <Row label="Diplomatic Standing" value={String(diplomacy.standing)} />
      <Row label="Consulates" value={String(diplomacy.consulates)} />
      <Row label="Embassies" value={String(diplomacy.embassies)} />

      {diplomacy.treaties.length > 0 && (
        <>
          <SectionLabel text="Active Treaties" />
          {diplomacy.treaties.map((t, i) => (
            <Row key={i} label={t.nation} value={t.treaty_type.replace(/([A-Z])/g, ' $1').trim()} />
          ))}
        </>
      )}

      {diplomacy.wars.length > 0 && (
        <>
          <SectionLabel text="At War With" />
          {diplomacy.wars.map((w, i) => (
            <Row key={i} label={w} value="At War" valueColor="#e63946" />
          ))}
        </>
      )}
    </div>
  );
}

function SectionLabel({ text }: { text: string }) {
  return <div style={styles.sectionLabel}>{text}</div>;
}

function Row({ label, value, highlight, valueColor }: { label: string; value: string; highlight?: boolean; valueColor?: string }) {
  return (
    <div style={highlight ? { ...styles.row, background: 'rgba(218,165,32,0.1)' } : styles.row}>
      <span style={styles.rowLabel}>{label}</span>
      <span style={{ ...styles.rowValue, color: valueColor || (highlight ? '#daa520' : '#ccc') }}>{value}</span>
    </div>
  );
}

function EmptyNote({ text }: { text: string }) {
  return <div style={{ padding: '4px 0', color: '#666', fontSize: 12, fontStyle: 'italic' }}>{text}</div>;
}

const styles: Record<string, React.CSSProperties> = {
  container: { padding: 0, height: '100%', display: 'flex', flexDirection: 'column' },
  title: { margin: '0 0 8px', fontSize: 18, fontFamily: 'Georgia, serif', color: '#daa520' },
  tabBar: { display: 'flex', gap: 2, marginBottom: 12, borderBottom: '1px solid #333' },
  tab: {
    background: 'transparent', border: 'none', color: '#888', padding: '6px 14px',
    cursor: 'pointer', fontFamily: 'Georgia, serif', fontSize: 13,
    borderBottom: '2px solid transparent',
  },
  tabActive: { color: '#daa520', borderBottom: '2px solid #daa520' },
  body: { flex: 1, overflowY: 'auto' },
  sectionLabel: {
    fontSize: 12, fontWeight: 'bold', color: '#daa520', textTransform: 'uppercase' as const,
    letterSpacing: 1, marginTop: 14, marginBottom: 4, borderBottom: '1px solid #333', paddingBottom: 2,
  },
  row: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '3px 0', fontSize: 13,
  },
  rowLabel: { color: '#aaa' },
  rowValue: { color: '#ccc', fontFamily: 'monospace' },
};
