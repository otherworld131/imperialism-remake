import React, { useState } from 'react';
import type { TradeData } from '../wasm';

interface Props {
  trade: TradeData | null;
  onSetSubsidy: (nationId: number, amount: number) => void;
  onSetSellOrder: (commodity: string, commodityType: string, quantity: number) => void;
  onSetBuyOrder: (resource: string, quantity: number, maxPrice: number) => void;
  onClose: () => void;
}

export default function TradeScreen({ trade, onSetSubsidy, onSetSellOrder, onSetBuyOrder, onClose }: Props) {
  const [buyModalResource, setBuyModalResource] = useState<string | null>(null);
  const [buyQuantity, setBuyQuantity] = useState(1);

  if (!trade) {
    return (
      <div style={styles.overlay}>
        <div style={styles.container}>
          <div style={styles.header}>
            <h2 style={styles.title}>Trade</h2>
            <button onClick={onClose} style={styles.closeBtn}>Esc</button>
          </div>
          <p style={{ padding: 24, color: '#999' }}>Loading trade data...</p>
        </div>
      </div>
    );
  }

  const { sellable_resources, sellable_materials, sellable_goods, available_offers, trade_balance, trade_history, subsidies, minor_nations, total_cargo, remaining_cargo } = trade;

  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        {/* Header */}
        <div style={styles.header}>
          <h2 style={styles.title}>Trade</h2>
          <div style={{ display: 'flex', gap: 20, alignItems: 'center' }}>
            <span>Cargo: {total_cargo - remaining_cargo} / {total_cargo}</span>
            <span>Imports: <span style={{ color: '#e63946' }}>${trade_balance.total_bought.toLocaleString()}</span></span>
            <span>Exports: <span style={{ color: '#2a9d8f' }}>${trade_balance.total_sold.toLocaleString()}</span></span>
            <span>Net: <span style={{ color: trade_balance.net >= 0 ? '#2a9d8f' : '#e63946' }}>${trade_balance.net.toLocaleString()}</span></span>
          </div>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>

        <div style={styles.body}>
          {/* Sell section */}
          <div style={styles.column}>
            <h3 style={styles.sectionTitle}>Sell Orders</h3>

            {[
              { label: 'Resources', items: sellable_resources, type: 'Resource' },
              { label: 'Materials', items: sellable_materials, type: 'Material' },
              { label: 'Goods', items: sellable_goods, type: 'Goods' },
            ].map(section => (
              <div key={section.label} style={{ marginBottom: 16 }}>
                <div style={styles.subLabel}>{section.label}</div>
                {section.items.map((item: any) => (
                  <div key={item.name} style={styles.tradeRow}>
                    <span style={styles.itemName}>{item.name}</span>
                    <span style={styles.stock}>x{item.stock}</span>
                    <span style={styles.price}>${item.price}</span>
                    <input
                      type="range"
                      min={0}
                      max={item.stock}
                      value={item.order_qty || 0}
                      onChange={e => onSetSellOrder(section.type.toLowerCase(), item.name, parseInt(e.target.value))}
                      style={styles.slider}
                    />
                    <span style={styles.qtyLabel}>{item.order_qty || 0}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>

          {/* Buy section */}
          <div style={styles.column}>
            <h3 style={styles.sectionTitle}>Buy Orders</h3>
            <div style={styles.subLabel}>Available on Market</div>
            <table style={styles.table}>
              <thead>
                <tr>
                  <th style={styles.th}>Resource</th>
                  <th style={styles.th}>Seller</th>
                  <th style={styles.th}>Avail</th>
                  <th style={styles.th}>Price</th>
                  <th style={styles.th}></th>
                </tr>
              </thead>
              <tbody>
                {available_offers.map((offer: any, i: number) => (
                  <tr key={i} style={{ opacity: offer.is_great_power ? 0.9 : 1 }}>
                    <td style={styles.td}>{offer.resource}</td>
                    <td style={{ ...styles.td, color: offer.is_great_power ? '#daa520' : '#999', fontSize: 11 }}>
                      {offer.seller_name}{offer.is_great_power ? ' (GP)' : ''}
                    </td>
                    <td style={styles.td}>{offer.quantity}</td>
                    <td style={styles.td}>${offer.price}</td>
                    <td style={styles.td}>
                      <button
                        onClick={() => { setBuyModalResource(offer.resource); setBuyQuantity(1); }}
                        style={styles.buyBtn}
                      >
                        Buy
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            {/* Trade partners / subsidies */}
            <h3 style={{ ...styles.sectionTitle, marginTop: 24 }}>Trade Partners</h3>
            {(minor_nations || []).map((mn: any) => {
              const sub = subsidies.find((s: any) => s.nation_id === mn.nation_id);
              const currentAmt = sub?.amount || 0;
              return (
                <div key={mn.nation_id} style={styles.partnerRow}>
                  <span>
                    {mn.name}
                    {mn.has_consulate && <span style={{ color: '#daa520', marginLeft: 4 }}>★</span>}
                  </span>
                  <div style={{ display: 'flex', gap: 4 }}>
                    {[0, 500, 1000, 2000].map(amt => (
                      <button
                        key={amt}
                        onClick={() => onSetSubsidy(mn.nation_id, amt)}
                        style={{
                          ...styles.subsidyBtn,
                          background: currentAmt === amt ? '#daa520' : '#3a3520',
                          color: currentAmt === amt ? '#000' : '#e0d8c0',
                        }}
                      >
                        ${amt}
                      </button>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Trade history */}
        <div style={styles.historySection}>
          <h3 style={styles.sectionTitle}>Trade History</h3>
          <div style={styles.historyScroll}>
            {trade_history.length === 0 && <span style={{ color: '#666' }}>No trades yet</span>}
            {trade_history.map((h: any, i: number) => (
              <div key={i} style={styles.historyRow}>
                <span>T{h.turn}</span>
                <span>{h.quantity}x {h.resource}</span>
                <span>{h.partner_name}</span>
                <span style={{ color: h.bought ? '#e63946' : '#2a9d8f' }}>
                  {h.bought ? '-' : '+'}${h.total_cost}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* Buy modal */}
        {buyModalResource && (
          <div style={styles.modal} onClick={() => setBuyModalResource(null)}>
            <div style={styles.modalContent} onClick={e => e.stopPropagation()}>
              <h3>Buy {buyModalResource}</h3>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12, margin: '16px 0' }}>
                <span>Quantity:</span>
                {(() => {
                  const offer = available_offers.find((o: any) => o.resource === buyModalResource);
                  const maxQty = Math.min(offer?.quantity || 1, remaining_cargo);
                  return (<>
                    <input
                      type="range"
                      min={1}
                      max={Math.max(1, maxQty)}
                      value={Math.min(buyQuantity, maxQty)}
                      onChange={e => setBuyQuantity(parseInt(e.target.value))}
                      style={{ flex: 1 }}
                    />
                    <span>{Math.min(buyQuantity, maxQty)} / {maxQty}</span>
                  </>);
                })()}
              </div>
              <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                <button onClick={() => setBuyModalResource(null)} style={styles.closeBtn}>Cancel</button>
                <button
                  onClick={() => {
                    const offer = available_offers.find((o: any) => o.resource === buyModalResource);
                    if (offer) {
                      onSetBuyOrder(buyModalResource, buyQuantity, Math.ceil(offer.price * 1.2));
                    }
                    setBuyModalResource(null);
                  }}
                  style={styles.buyConfirmBtn}
                >
                  Buy {buyQuantity} for ~${(() => {
                    const offer = available_offers.find((o: any) => o.resource === buyModalResource);
                    return offer ? (buyQuantity * offer.price).toLocaleString() : '?';
                  })()}
                </button>
              </div>
            </div>
          </div>
        )}
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
    display: 'flex', flex: 1, minHeight: 0, overflow: 'hidden',
  },
  column: {
    flex: 1, padding: '16px 24px', overflowY: 'auto' as const,
    borderRight: '1px solid #3a3520',
  },
  sectionTitle: {
    color: '#daa520', margin: '0 0 12px', fontSize: 16,
    borderBottom: '1px solid #3a3520', paddingBottom: 6,
  },
  subLabel: {
    fontSize: 11, textTransform: 'uppercase' as const, letterSpacing: 1,
    color: '#888', marginBottom: 8,
  },
  tradeRow: {
    display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0', fontSize: 13,
  },
  itemName: { width: 100, flexShrink: 0 },
  stock: { width: 40, color: '#999', fontSize: 12 },
  price: { width: 50, color: '#daa520', fontSize: 12 },
  slider: { flex: 1, cursor: 'pointer' },
  qtyLabel: { width: 30, textAlign: 'right' as const, fontWeight: 'bold' },
  table: { width: '100%', borderCollapse: 'collapse' as const, fontSize: 13 },
  th: { textAlign: 'left' as const, padding: '6px 8px', borderBottom: '1px solid #3a3520', color: '#daa520', fontSize: 11, textTransform: 'uppercase' as const },
  td: { padding: '6px 8px', borderBottom: '1px solid #1a1a2e' },
  buyBtn: {
    padding: '2px 10px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', cursor: 'pointer', fontSize: 11,
    fontFamily: "'Georgia', serif",
  },
  partnerRow: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '4px 0', fontSize: 13,
  },
  subsidyBtn: {
    padding: '2px 6px', border: '1px solid #5a5030', cursor: 'pointer',
    fontSize: 10, fontFamily: "'Georgia', serif",
  },
  historySection: {
    padding: '12px 24px', borderTop: '2px solid #3a3520',
    maxHeight: 150, overflow: 'hidden',
  },
  historyScroll: { overflowY: 'auto' as const, maxHeight: 100 },
  historyRow: {
    display: 'flex', gap: 16, fontSize: 12, padding: '2px 0', color: '#999',
  },
  modal: {
    position: 'fixed' as const, inset: 0, background: 'rgba(0,0,0,0.7)',
    display: 'flex', justifyContent: 'center', alignItems: 'center', zIndex: 200,
  },
  modalContent: {
    background: '#1a1a2e', border: '2px solid #daa520', padding: 24, minWidth: 300,
  },
  buyConfirmBtn: {
    padding: '6px 16px', background: '#8b4513', color: '#fff',
    border: 'none', cursor: 'pointer', fontWeight: 'bold', fontFamily: "'Georgia', serif",
  },
};
