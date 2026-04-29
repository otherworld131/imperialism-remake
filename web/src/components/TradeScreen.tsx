import React, { useState, useMemo } from 'react';
import type { TradeData } from '../wasm';
import { resourceLabel } from '../resourceEmoji';
import Flag from './Flag';

interface NationLite {
  id: number;
  flag_svg?: string;
  name?: string;
}

interface Props {
  trade: TradeData | null;
  nations?: NationLite[];
  onSetSubsidy: (nationId: number, amount: number) => void;
  onSetSellOrder: (commodity: string, commodityType: string, quantity: number) => void;
  onSetBuyOrder: (resource: string, quantity: number, maxPrice: number) => void;
  onClose: () => void;
}

type TradeTab = 'orders' | 'summary';

export default function TradeScreen({ trade, nations = [], onSetSubsidy, onSetSellOrder, onSetBuyOrder, onClose }: Props) {
  const flagBySellerId: Record<number, string> = {};
  for (const n of nations) {
    if (n.flag_svg) flagBySellerId[n.id] = n.flag_svg;
  }
  const [buyModalResource, setBuyModalResource] = useState<string | null>(null);
  const [buyQuantity, setBuyQuantity] = useState(1);
  const [activeTab, setActiveTab] = useState<TradeTab>('orders');
  const [selectedSummaryTurn, setSelectedSummaryTurn] = useState<number | null>(null);

  const tradeHistory: any[] = trade?.trade_history ?? [];

  // Group trade history by turn, sorted newest first
  const turnsSorted = useMemo(() => {
    const turns = Array.from(new Set(tradeHistory.map((h: any) => h.turn as number)));
    return turns.sort((a, b) => b - a);
  }, [tradeHistory]);

  const activeSummaryTurn = selectedSummaryTurn ?? (turnsSorted[0] ?? null);

  // Trades for the selected summary turn, grouped by resource
  const summaryByResource = useMemo(() => {
    const relevant = tradeHistory.filter((h: any) => h.turn === activeSummaryTurn);
    const map: Record<string, { bought: number; sold: number; boughtCost: number; soldCost: number; partners: Set<string> }> = {};
    for (const h of relevant) {
      if (!map[h.resource]) map[h.resource] = { bought: 0, sold: 0, boughtCost: 0, soldCost: 0, partners: new Set() };
      if (h.bought) {
        map[h.resource].bought += h.quantity;
        map[h.resource].boughtCost += h.total_cost;
      } else {
        map[h.resource].sold += h.quantity;
        map[h.resource].soldCost += h.total_cost;
      }
      map[h.resource].partners.add(h.partner_name);
    }
    return Object.entries(map).sort((a, b) => a[0].localeCompare(b[0]));
  }, [tradeHistory, activeSummaryTurn]);

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
          <div style={styles.modeTabs}>
            <button
              style={activeTab === 'orders' ? styles.modeTabActive : styles.modeTab}
              onClick={() => setActiveTab('orders')}
            >Orders</button>
            <button
              style={activeTab === 'summary' ? styles.modeTabActive : styles.modeTab}
              onClick={() => setActiveTab('summary')}
            >Summary</button>
          </div>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>

        {activeTab === 'orders' && (
          <>
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
                        <span style={styles.itemName}>{resourceLabel(item.name)}</span>
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
                        <td style={styles.td}>{resourceLabel(offer.resource)}</td>
                        <td style={{ ...styles.td, color: offer.is_great_power ? '#daa520' : '#999', fontSize: 11 }}>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                            {flagBySellerId[offer.seller_id] && (
                              <Flag svg={flagBySellerId[offer.seller_id]} width={18} height={12} title={offer.seller_name} />
                            )}
                            {offer.seller_name}{offer.is_great_power ? ' (GP)' : ''}
                          </span>
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
                    <span>{h.quantity}x {resourceLabel(h.resource)}</span>
                    <span>{h.partner_name}</span>
                    <span style={{ color: h.bought ? '#e63946' : '#2a9d8f' }}>
                      {h.bought ? '-' : '+'}${h.total_cost}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </>
        )}

        {activeTab === 'summary' && (
          <div style={styles.summaryBody}>
            {/* Turn sidebar */}
            <div style={styles.summarySidebar}>
              <div style={styles.summaryArchiveTitle}>Past Turns</div>
              {turnsSorted.length === 0 && (
                <p style={{ color: '#666', fontSize: 12, padding: '8px 0' }}>No trade history yet.</p>
              )}
              {turnsSorted.map(t => {
                const count = trade_history.filter((h: any) => h.turn === t).length;
                return (
                  <button
                    key={t}
                    style={t === activeSummaryTurn ? styles.summaryItemActive : styles.summaryItem}
                    onClick={() => setSelectedSummaryTurn(t)}
                  >
                    Turn {t}
                    <span style={styles.summaryBadge}>{count}</span>
                  </button>
                );
              })}
            </div>

            {/* Summary content */}
            <div style={styles.summaryContent}>
              {activeSummaryTurn === null ? (
                <p style={{ color: '#666', padding: 24 }}>No trade data available.</p>
              ) : (
                <>
                  <h3 style={{ ...styles.sectionTitle, marginBottom: 16 }}>
                    Turn {activeSummaryTurn} — Trade by Resource
                  </h3>
                  {summaryByResource.length === 0 ? (
                    <p style={{ color: '#666' }}>No trades recorded for this turn.</p>
                  ) : (
                    <table style={{ ...styles.table, maxWidth: 700 }}>
                      <thead>
                        <tr>
                          <th style={{ ...styles.th, textAlign: 'left' as const }}>Resource</th>
                          <th style={styles.th}>Bought</th>
                          <th style={styles.th}>Cost</th>
                          <th style={styles.th}>Sold</th>
                          <th style={styles.th}>Revenue</th>
                          <th style={{ ...styles.th, textAlign: 'left' as const }}>Partners</th>
                        </tr>
                      </thead>
                      <tbody>
                        {summaryByResource.map(([resource, data]) => (
                          <tr key={resource} style={{ borderBottom: '1px solid #1a1a2e' }}>
                            <td style={{ ...styles.td, textAlign: 'left' as const, color: '#daa520' }}>{resourceLabel(resource)}</td>
                            <td style={{ ...styles.td, color: data.bought > 0 ? '#e63946' : '#555' }}>
                              {data.bought > 0 ? data.bought : '—'}
                            </td>
                            <td style={{ ...styles.td, color: data.bought > 0 ? '#e63946' : '#555' }}>
                              {data.bought > 0 ? `$${data.boughtCost.toLocaleString()}` : '—'}
                            </td>
                            <td style={{ ...styles.td, color: data.sold > 0 ? '#2a9d8f' : '#555' }}>
                              {data.sold > 0 ? data.sold : '—'}
                            </td>
                            <td style={{ ...styles.td, color: data.sold > 0 ? '#2a9d8f' : '#555' }}>
                              {data.sold > 0 ? `$${data.soldCost.toLocaleString()}` : '—'}
                            </td>
                            <td style={{ ...styles.td, textAlign: 'left' as const, color: '#999', fontSize: 11 }}>
                              {Array.from(data.partners).join(', ')}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </>
              )}
            </div>
          </div>
        )}

        {/* Buy modal */}
        {buyModalResource && (
          <div style={styles.modal} onClick={() => setBuyModalResource(null)}>
            <div style={styles.modalContent} onClick={e => e.stopPropagation()}>
              <h3>Buy {resourceLabel(buyModalResource)}</h3>
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
    background: '#0f0f23', gap: 16, flexWrap: 'wrap' as const,
  },
  title: { color: '#daa520', margin: 0, fontSize: 22 },
  closeBtn: {
    padding: '4px 12px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', cursor: 'pointer', fontFamily: "'Georgia', serif",
  },
  modeTabs: { display: 'flex', gap: 4 },
  modeTab: {
    padding: '4px 14px', background: 'transparent', border: '1px solid #5a5030',
    color: '#888', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)', borderRadius: 4,
  },
  modeTabActive: {
    padding: '4px 14px', background: 'rgba(218,165,32,0.12)', border: '1px solid #daa520',
    color: '#daa520', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)', borderRadius: 4,
  },
  summaryBody: {
    display: 'flex', flex: 1, minHeight: 0, overflow: 'hidden',
  },
  summarySidebar: {
    width: 140, background: '#0f0f23', borderRight: '1px solid #3a3520',
    padding: '12px 8px', overflowY: 'auto' as const, flexShrink: 0,
    display: 'flex', flexDirection: 'column' as const, gap: 4,
  },
  summaryArchiveTitle: {
    fontSize: 10, textTransform: 'uppercase' as const, letterSpacing: 1,
    color: '#888', marginBottom: 6, padding: '0 4px',
  },
  summaryItem: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '5px 8px', background: 'transparent', border: '1px solid #333',
    color: '#999', cursor: 'pointer', fontSize: 'var(--ui-font-size, 14px)', fontFamily: "'Georgia', serif",
    borderRadius: 3, textAlign: 'left' as const,
  },
  summaryItemActive: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '5px 8px', background: 'rgba(218,165,32,0.12)', border: '1px solid #daa520',
    color: '#daa520', cursor: 'pointer', fontSize: 'var(--ui-font-size, 14px)', fontFamily: "'Georgia', serif",
    borderRadius: 3, textAlign: 'left' as const,
  },
  summaryBadge: {
    background: '#3a3520', borderRadius: 10, padding: '1px 5px', fontSize: 10, color: '#bbb',
  },
  summaryContent: {
    flex: 1, padding: '16px 24px', overflowY: 'auto' as const,
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
    display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0', fontSize: 'var(--ui-font-size, 14px)',
  },
  itemName: { width: 100, flexShrink: 0 },
  stock: { width: 40, color: '#999', fontSize: 'var(--ui-font-size, 14px)' },
  price: { width: 50, color: '#daa520', fontSize: 'var(--ui-font-size, 14px)' },
  slider: { flex: 1, cursor: 'pointer' },
  qtyLabel: { width: 30, textAlign: 'right' as const, fontWeight: 'bold' },
  table: { width: '100%', borderCollapse: 'collapse' as const, fontSize: 'var(--ui-font-size, 14px)' },
  th: { textAlign: 'left' as const, padding: '6px 8px', borderBottom: '1px solid #3a3520', color: '#daa520', fontSize: 11, textTransform: 'uppercase' as const },
  td: { padding: '6px 8px', borderBottom: '1px solid #1a1a2e' },
  buyBtn: {
    padding: '2px 10px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', cursor: 'pointer', fontSize: 11,
    fontFamily: "'Georgia', serif",
  },
  partnerRow: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '4px 0', fontSize: 'var(--ui-font-size, 14px)',
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
    display: 'flex', gap: 16, fontSize: 'var(--ui-font-size, 14px)', padding: '2px 0', color: '#999',
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
