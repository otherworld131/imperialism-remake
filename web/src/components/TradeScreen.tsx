import React, { useState, useMemo, useCallback } from 'react';
import type { TradeData, ShipDetail } from '../wasm';
import { resourceLabel, resourceEmoji, commodityCategory } from '../resourceEmoji';
import Flag from './Flag';
import FilterDropdown, { type FilterOption } from './FilterDropdown';

interface NationLite {
  id: number;
  flag_svg?: string;
  name?: string;
}

interface Props {
  trade: TradeData | null;
  nations?: NationLite[];
  /** Player's merchant fleet (card #472). Drives the cargo tooltip; a missing
   *  list just means no breakdown is shown. */
  merchants?: ShipDetail[];
  onSetSubsidy: (nationId: number, amount: number) => void;
  onSetSellOrder: (commodity: string, commodityType: string, quantity: number) => void;
  onSetBuyOrder: (resource: string, quantity: number, maxPrice: number) => void;
  onSetAutoTradeWithMinors: (enabled: boolean) => void;
  onClose: () => void;
}

type TradeTab = 'orders' | 'historical_country' | 'historical_market';
type OfferSortKey = 'resource' | 'seller_name' | 'quantity' | 'price' | 'is_great_power';
const OFFER_COLS: { key: OfferSortKey; label: string }[] = [
  { key: 'resource', label: 'Item' },
  { key: 'seller_name', label: 'Seller' },
  { key: 'quantity', label: 'Avail' },
  { key: 'price', label: 'Price' },
  { key: 'is_great_power', label: 'GP' },
];
type HistSortKey = 'turn' | 'resource' | 'quantity' | 'total_cost' | 'partner_name' | 'bought' | 'partner_is_great_power';
const HIST_COLS_SPLIT: { key: HistSortKey; label: string }[] = [
  { key: 'turn', label: 'Turn' },
  { key: 'resource', label: 'Item' },
  { key: 'bought', label: 'B/S' },
  { key: 'quantity', label: 'Qty' },
  { key: 'total_cost', label: 'Cost' },
  { key: 'partner_name', label: 'Partner' },
  { key: 'partner_is_great_power', label: 'GP' },
];
type AggSortKey = 'turn' | 'resource' | 'partner_name' | 'bought' | 'sold' | 'boughtCost' | 'soldCost';
const HIST_COLS_AGG: { key: AggSortKey; label: string }[] = [
  { key: 'turn', label: 'Turn' },
  { key: 'resource', label: 'Item' },
  { key: 'partner_name', label: 'Partner' },
  { key: 'bought', label: 'Bought' },
  { key: 'boughtCost', label: 'Cost' },
  { key: 'sold', label: 'Sold' },
  { key: 'soldCost', label: 'Revenue' },
];

type MarketSortKey = 'resource' | 'seller_name' | 'offered' | 'sold' | 'price_per_unit';
const MARKET_COLS: { key: MarketSortKey; label: string }[] = [
  { key: 'resource', label: 'Item' },
  { key: 'seller_name', label: 'Seller' },
  { key: 'offered', label: 'Offered' },
  { key: 'sold', label: 'Sold' },
  { key: 'price_per_unit', label: 'Price' },
];

export default function TradeScreen({ trade, nations = [], merchants = [], onSetSubsidy, onSetSellOrder, onSetBuyOrder, onSetAutoTradeWithMinors, onClose }: Props) {
  const flagBySellerId: Record<number, string> = {};
  for (const n of nations) {
    if (n.flag_svg) flagBySellerId[n.id] = n.flag_svg;
  }
  const [buyModalResource, setBuyModalResource] = useState<string | null>(null);
  const [buyQuantity, setBuyQuantity] = useState(1);
  const [activeTab, setActiveTab] = useState<TradeTab>('orders');
  const [offerSort, setOfferSort] = useState<{ key: OfferSortKey; dir: 1 | -1 }[]>([{ key: 'resource', dir: 1 }]);
  const cycleOfferSort = useCallback((key: OfferSortKey) => {
    setOfferSort(prev => {
      const idx = prev.findIndex(s => s.key === key);
      if (idx === -1) return [...prev, { key, dir: 1 }];
      if (prev[idx].dir === 1) return prev.map((s, i) => i === idx ? { key, dir: -1 } : s);
      return prev.filter((_, i) => i !== idx);
    });
  }, []);
  const [histSort, setHistSort] = useState<{ key: HistSortKey; dir: 1 | -1 }[]>([{ key: 'turn', dir: -1 }]);
  const cycleHistSort = useCallback((key: HistSortKey) => {
    setHistSort(prev => {
      const idx = prev.findIndex(s => s.key === key);
      if (idx === -1) return [...prev, { key, dir: 1 }];
      if (prev[idx].dir === 1) return prev.map((s, i) => i === idx ? { key, dir: -1 } : s);
      return prev.filter((_, i) => i !== idx);
    });
  }, []);
  const [aggSort, setAggSort] = useState<{ key: AggSortKey; dir: 1 | -1 }[]>([{ key: 'turn', dir: -1 }]);
  const cycleAggSort = useCallback((key: AggSortKey) => {
    setAggSort(prev => {
      const idx = prev.findIndex(s => s.key === key);
      if (idx === -1) return [...prev, { key, dir: 1 }];
      if (prev[idx].dir === 1) return prev.map((s, i) => i === idx ? { key, dir: -1 } : s);
      return prev.filter((_, i) => i !== idx);
    });
  }, []);
  const [histSplit, setHistSplit] = useState(false);
  const [selectedSummaryTurn, setSelectedSummaryTurn] = useState<number | null>(null);
  const [marketSort, setMarketSort] = useState<{ key: MarketSortKey; dir: 1 | -1 }[]>([
    { key: 'resource', dir: 1 },
  ]);
  const cycleMarketSort = useCallback((key: MarketSortKey) => {
    setMarketSort(prev => {
      const idx = prev.findIndex(s => s.key === key);
      if (idx === -1) return [...prev, { key, dir: 1 }];
      if (prev[idx].dir === 1) return prev.map((s, i) => i === idx ? { key, dir: -1 } : s);
      return prev.filter((_, i) => i !== idx);
    });
  }, []);
  const [selectedMarketTurn, setSelectedMarketTurn] = useState<number | null>(null);

  // Per-table multi-select filters (commodity + country involved in the exchange).
  // The country filter matches sellers/partners — for the market table it also matches any buyer in the fills.
  // Empty array = no filter (show everything).
  const [offerCommodity, setOfferCommodity] = useState<string[]>([]);
  const [offerCountry, setOfferCountry] = useState<string[]>([]);
  const [histCommodity, setHistCommodity] = useState<string[]>([]);
  const [histCountry, setHistCountry] = useState<string[]>([]);
  const [marketCommodity, setMarketCommodity] = useState<string[]>([]);
  const [marketCountry, setMarketCountry] = useState<string[]>([]);

  const tradeHistory: any[] = trade?.trade_history ?? [];
  const marketArchive: any[] = (trade as any)?.market_archive ?? [];

  // Group trade history by turn, sorted newest first
  const turnsSorted = useMemo(() => {
    const turns = Array.from(new Set(tradeHistory.map((h: any) => h.turn as number)));
    return turns.sort((a, b) => b - a);
  }, [tradeHistory]);

  const filteredOffers = useMemo(() => {
    if (!trade) return [];
    return trade.available_offers.filter((o: any) =>
      (offerCommodity.length === 0 || offerCommodity.includes(o.resource)) &&
      (offerCountry.length === 0 || offerCountry.includes(String(o.seller_id)))
    );
  }, [trade, offerCommodity, offerCountry]);

  const sortedOffers = useMemo(() => {
    return [...filteredOffers].sort((a: any, b: any) => {
      for (const { key, dir } of offerSort) {
        const av = a[key], bv = b[key];
        const cmp = typeof av === 'boolean' ? (av === bv ? 0 : av ? -1 : 1)
          : typeof av === 'number' ? av - bv
          : String(av).localeCompare(String(bv));
        if (cmp !== 0) return cmp * dir;
      }
      return 0;
    });
  }, [filteredOffers, offerSort]);

  const offerCommodityOptions = useMemo<FilterOption[]>(() => {
    const names = new Set<string>();
    for (const o of trade?.available_offers ?? []) names.add((o as any).resource);
    return Array.from(names).sort().map(n => ({ value: n, label: n, emoji: resourceEmoji(n) }));
  }, [trade]);

  const offerCountryOptions = useMemo<FilterOption[]>(() => {
    const map = new Map<number, string>();
    for (const o of trade?.available_offers ?? []) map.set((o as any).seller_id, (o as any).seller_name);
    return Array.from(map.entries())
      .sort((a, b) => a[1].localeCompare(b[1]))
      .map(([id, name]) => ({ value: String(id), label: name, flagSvg: flagBySellerId[id] }));
  }, [trade, flagBySellerId]);

  const offerCommodityGroups = useMemo(() => partitionCommodities(offerCommodityOptions), [offerCommodityOptions]);
  const offerCountryGroups = useMemo(() => {
    const gp = new Set<number>();
    for (const o of trade?.available_offers ?? []) if ((o as any).is_great_power) gp.add((o as any).seller_id);
    return partitionCountries(offerCountryOptions, gp);
  }, [trade, offerCountryOptions]);

  const filteredHistory = useMemo(() => {
    return tradeHistory.filter((h: any) =>
      (histCommodity.length === 0 || histCommodity.includes(h.resource)) &&
      (histCountry.length === 0 || histCountry.includes(String(h.partner_id)))
    );
  }, [tradeHistory, histCommodity, histCountry]);

  const histCommodityOptions = useMemo<FilterOption[]>(() => {
    const names = new Set<string>();
    for (const h of tradeHistory) names.add(h.resource);
    return Array.from(names).sort().map(n => ({ value: n, label: n, emoji: resourceEmoji(n) }));
  }, [tradeHistory]);

  const histCountryOptions = useMemo<FilterOption[]>(() => {
    const map = new Map<number, string>();
    for (const h of tradeHistory) {
      if (h.partner_id != null) map.set(h.partner_id, h.partner_name ?? 'Unknown');
    }
    return Array.from(map.entries())
      .sort((a, b) => a[1].localeCompare(b[1]))
      .map(([id, name]) => ({ value: String(id), label: name, flagSvg: flagBySellerId[id] }));
  }, [tradeHistory, flagBySellerId]);

  const histCommodityGroups = useMemo(() => partitionCommodities(histCommodityOptions), [histCommodityOptions]);
  const histCountryGroups = useMemo(() => {
    const gp = new Set<number>();
    for (const h of tradeHistory) if (h.partner_is_great_power && h.partner_id != null) gp.add(h.partner_id);
    return partitionCountries(histCountryOptions, gp);
  }, [tradeHistory, histCountryOptions]);

  const sortedHistory = useMemo(() => {
    return [...filteredHistory].sort((a: any, b: any) => {
      for (const { key, dir } of histSort) {
        const av = a[key], bv = b[key];
        const cmp = typeof av === 'boolean' ? (av === bv ? 0 : av ? -1 : 1)
          : typeof av === 'number' ? av - bv
          : String(av).localeCompare(String(bv));
        if (cmp !== 0) return cmp * dir;
      }
      return 0;
    });
  }, [filteredHistory, histSort]);

  const aggregatedHistory = useMemo(() => {
    const filtered = (selectedSummaryTurn === null ? filteredHistory : filteredHistory.filter((h: any) => h.turn === selectedSummaryTurn));
    type Row = {
      turn: number;
      resource: string;
      partner_id: number;
      partner_name: string;
      partner_is_great_power: boolean;
      bought: number;
      sold: number;
      boughtCost: number;
      soldCost: number;
    };
    const map: Record<string, Row> = {};
    for (const h of filtered) {
      const k = `${h.turn}__${h.resource}__${h.partner_id ?? -1}`;
      if (!map[k]) {
        map[k] = {
          turn: h.turn,
          resource: h.resource,
          partner_id: h.partner_id ?? 0,
          partner_name: h.partner_name ?? 'Unknown',
          partner_is_great_power: !!h.partner_is_great_power,
          bought: 0, sold: 0, boughtCost: 0, soldCost: 0,
        };
      }
      if (h.bought) { map[k].bought += h.quantity; map[k].boughtCost += h.total_cost; }
      else { map[k].sold += h.quantity; map[k].soldCost += h.total_cost; }
    }
    const rows = Object.values(map);
    return rows.sort((a, b) => {
      for (const { key, dir } of aggSort) {
        const av = (a as any)[key], bv = (b as any)[key];
        const cmp = typeof av === 'number' ? av - bv : String(av).localeCompare(String(bv));
        if (cmp !== 0) return cmp * dir;
      }
      return 0;
    });
  }, [filteredHistory, selectedSummaryTurn, aggSort]);

  // Market history: turns available, newest first.
  const marketTurnsSorted = useMemo(() => {
    return marketArchive
      .map((rec: any) => rec.turn as number)
      .sort((a, b) => b - a);
  }, [marketArchive]);

  // Sorted offer rows for the currently-selected market turn (or the latest if
  // none is explicitly selected). Returns [] if archive is empty.
  const currentMarketRec = useMemo(() => {
    if (marketArchive.length === 0) return null;
    const turn = selectedMarketTurn ?? marketTurnsSorted[0];
    return marketArchive.find((r: any) => r.turn === turn) ?? null;
  }, [marketArchive, marketTurnsSorted, selectedMarketTurn]);

  const marketCommodityOptions = useMemo<FilterOption[]>(() => {
    const names = new Set<string>();
    for (const o of (currentMarketRec?.offers ?? [])) names.add(o.resource);
    return Array.from(names).sort().map(n => ({ value: n, label: n, emoji: resourceEmoji(n) }));
  }, [currentMarketRec]);

  const marketCountryOptions = useMemo<FilterOption[]>(() => {
    const map = new Map<number, string>();
    for (const o of (currentMarketRec?.offers ?? [])) {
      if (o.seller_id != null) map.set(o.seller_id, o.seller_name ?? 'Unknown');
      for (const f of (o.fills ?? [])) {
        if (f.buyer_id != null) map.set(f.buyer_id, f.buyer_name ?? 'Unknown');
      }
    }
    return Array.from(map.entries())
      .sort((a, b) => a[1].localeCompare(b[1]))
      .map(([id, name]) => ({ value: String(id), label: name, flagSvg: flagBySellerId[id] }));
  }, [currentMarketRec, flagBySellerId]);

  const marketCommodityGroups = useMemo(() => partitionCommodities(marketCommodityOptions), [marketCommodityOptions]);
  const marketCountryGroups = useMemo(() => {
    const gp = new Set<number>();
    for (const o of (currentMarketRec?.offers ?? [])) {
      if (o.seller_is_great_power && o.seller_id != null) gp.add(o.seller_id);
      for (const f of (o.fills ?? [])) {
        if (f.buyer_is_great_power && f.buyer_id != null) gp.add(f.buyer_id);
      }
    }
    return partitionCountries(marketCountryOptions, gp);
  }, [currentMarketRec, marketCountryOptions]);

  const sortedMarketOffers = useMemo(() => {
    if (!currentMarketRec) return [] as any[];
    const rows: any[] = (currentMarketRec.offers ?? []).filter((o: any) =>
      (marketCommodity.length === 0 || marketCommodity.includes(o.resource)) &&
      (marketCountry.length === 0
        || marketCountry.includes(String(o.seller_id))
        || (o.fills ?? []).some((f: any) => marketCountry.includes(String(f.buyer_id))))
    );
    return rows.sort((a, b) => {
      for (const { key, dir } of marketSort) {
        const av = (a as any)[key], bv = (b as any)[key];
        const cmp = typeof av === 'number' ? av - bv : String(av).localeCompare(String(bv));
        if (cmp !== 0) return cmp * dir;
      }
      return 0;
    });
  }, [currentMarketRec, marketSort, marketCommodity, marketCountry]);

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

  const { sellable_resources, available_offers, trade_balance, trade_history, subsidies, minor_nations, total_cargo, remaining_cargo } = trade;

  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        {/* Header */}
        <div style={styles.header}>
          <h2 style={styles.title}>Trade</h2>
          <div style={{ display: 'flex', gap: 20, alignItems: 'center' }}>
            <CargoSummary
              used={total_cargo - remaining_cargo}
              total={total_cargo}
              merchants={merchants}
            />
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
              style={activeTab === 'historical_country' ? styles.modeTabActive : styles.modeTab}
              onClick={() => setActiveTab('historical_country')}
            >Historical Country</button>
            <button
              style={activeTab === 'historical_market' ? styles.modeTabActive : styles.modeTab}
              onClick={() => setActiveTab('historical_market')}
            >Historical Market</button>
          </div>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>

        {activeTab === 'orders' && (
          <>
            <div style={styles.body}>
              {/* Sell section */}
              <div style={styles.column}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                  <h3 style={{ ...styles.sectionTitle, margin: 0 }}>Sell Orders</h3>
                  <button
                    onClick={() => onSetAutoTradeWithMinors(!(trade?.auto_trade_with_minors ?? true))}
                    style={{
                      background: (trade?.auto_trade_with_minors ?? true) ? '#2a4a2a' : '#4a2a2a',
                      color: (trade?.auto_trade_with_minors ?? true) ? '#6d6' : '#d66',
                      border: `1px solid ${(trade?.auto_trade_with_minors ?? true) ? '#4a7a4a' : '#7a4a4a'}`,
                      borderRadius: 4, padding: '2px 8px', fontSize: 10, cursor: 'pointer',
                    }}
                    title="When enabled, minor nations may automatically buy your goods each turn"
                  >
                    🤝 Minor auto-buy: {(trade?.auto_trade_with_minors ?? true) ? 'ON' : 'OFF'}
                  </button>
                </div>

                {[
                  { label: 'Resources', items: sellable_resources, type: 'Resource' },
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
                <div style={styles.filterRow}>
                  <FilterDropdown
                    values={offerCommodity}
                    options={offerCommodityOptions}
                    onChange={setOfferCommodity}
                    placeholder="All commodities"
                  />
                  <QuickChip label="Goods" groupValues={offerCommodityGroups.goods} values={offerCommodity} onChange={setOfferCommodity} />
                  <QuickChip label="Materials" groupValues={offerCommodityGroups.materials} values={offerCommodity} onChange={setOfferCommodity} />
                  <QuickChip label="Resources" groupValues={offerCommodityGroups.resources} values={offerCommodity} onChange={setOfferCommodity} />
                  <span style={styles.filterDivider} />
                  <FilterDropdown
                    values={offerCountry}
                    options={offerCountryOptions}
                    onChange={setOfferCountry}
                    placeholder="All sellers"
                  />
                  <QuickChip label="Great Powers" groupValues={offerCountryGroups.great} values={offerCountry} onChange={setOfferCountry} />
                  <QuickChip label="Minor Powers" groupValues={offerCountryGroups.minor} values={offerCountry} onChange={setOfferCountry} />
                </div>
                <table style={styles.table}>
                  <thead>
                    <tr>
                      {OFFER_COLS.map(col => (
                        <th
                          key={col.key}
                          style={{ ...styles.th, cursor: 'pointer', userSelect: 'none' as const }}
                          onClick={() => cycleOfferSort(col.key)}
                        >
                          {col.label}{(() => { const s = offerSort.find(x => x.key === col.key); if (!s) return ''; const rank = offerSort.length > 1 ? String(offerSort.indexOf(s) + 1) : ''; return (s.dir === 1 ? ' ▲' : ' ▼') + rank; })()}
                        </th>
                      ))}
                      <th style={styles.th}></th>
                    </tr>
                  </thead>
                  <tbody>
                    {sortedOffers.map((offer: any, i: number) => (
                      <tr key={i} style={{ opacity: offer.is_great_power ? 0.9 : 1 }}>
                        <td style={styles.td}>{resourceLabel(offer.resource)}</td>
                        <td style={{ ...styles.td, color: offer.is_great_power ? '#daa520' : '#999', fontSize: 11 }}>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                            {flagBySellerId[offer.seller_id] && (
                              <Flag svg={flagBySellerId[offer.seller_id]} width={18} height={12} title={offer.seller_name} />
                            )}
                            {offer.seller_name}
                          </span>
                        </td>
                        <td style={styles.td}>{offer.quantity}</td>
                        <td style={styles.td}>${offer.price}</td>
                        <td style={{ ...styles.td, color: '#daa520', textAlign: 'center' as const }}>
                          {offer.is_great_power ? '★' : ''}
                        </td>
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

          </>
        )}

        {activeTab === 'historical_country' && (
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
                    style={t === selectedSummaryTurn ? styles.summaryItemActive : styles.summaryItem}
                    onClick={() => setSelectedSummaryTurn(prev => prev === t ? null : t)}
                  >
                    Turn {t}
                    <span style={styles.summaryBadge}>{count}</span>
                  </button>
                );
              })}
            </div>

            {/* Historical content */}
            <div style={styles.summaryContent}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12, flexWrap: 'wrap' as const }}>
                <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: '#999', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={histSplit}
                    onChange={e => setHistSplit(e.target.checked)}
                    style={{ cursor: 'pointer' }}
                  />
                  Split individual transactions
                </label>
                <FilterDropdown
                  values={histCommodity}
                  options={histCommodityOptions}
                  onChange={setHistCommodity}
                  placeholder="All commodities"
                />
                <QuickChip label="Goods" groupValues={histCommodityGroups.goods} values={histCommodity} onChange={setHistCommodity} />
                <QuickChip label="Materials" groupValues={histCommodityGroups.materials} values={histCommodity} onChange={setHistCommodity} />
                <QuickChip label="Resources" groupValues={histCommodityGroups.resources} values={histCommodity} onChange={setHistCommodity} />
                <span style={styles.filterDivider} />
                <FilterDropdown
                  values={histCountry}
                  options={histCountryOptions}
                  onChange={setHistCountry}
                  placeholder="All partners"
                />
                <QuickChip label="Great Powers" groupValues={histCountryGroups.great} values={histCountry} onChange={setHistCountry} />
                <QuickChip label="Minor Powers" groupValues={histCountryGroups.minor} values={histCountry} onChange={setHistCountry} />
              </div>
              {trade_history.length === 0 ? (
                <p style={{ color: '#666' }}>No trade data available.</p>
              ) : histSplit ? (
                <table style={styles.table}>
                  <thead>
                    <tr>
                      {HIST_COLS_SPLIT.map(col => (
                        <th
                          key={col.key}
                          style={{ ...styles.th, cursor: 'pointer', userSelect: 'none' as const }}
                          onClick={() => cycleHistSort(col.key)}
                        >
                          {col.label}{(() => { const s = histSort.find(x => x.key === col.key); if (!s) return ''; const rank = histSort.length > 1 ? String(histSort.indexOf(s) + 1) : ''; return (s.dir === 1 ? ' ▲' : ' ▼') + rank; })()}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {sortedHistory
                      .filter((h: any) => selectedSummaryTurn === null || h.turn === selectedSummaryTurn)
                      .map((h: any, i: number) => (
                        <tr key={i}>
                          <td style={styles.td}>{h.turn}</td>
                          <td style={{ ...styles.td, color: '#daa520' }}>{resourceLabel(h.resource)}</td>
                          <td style={{ ...styles.td, color: h.bought ? '#e63946' : '#2a9d8f' }}>{h.bought ? 'Buy' : 'Sell'}</td>
                          <td style={styles.td}>{h.quantity}</td>
                          <td style={{ ...styles.td, color: h.bought ? '#e63946' : '#2a9d8f' }}>
                            {h.bought ? '-' : '+'}${h.total_cost.toLocaleString()}
                          </td>
                          <td style={{ ...styles.td, fontSize: 11 }}>
                            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                              {flagBySellerId[h.partner_id] && (
                                <Flag svg={flagBySellerId[h.partner_id]} width={18} height={12} title={h.partner_name} />
                              )}
                              <span style={{ color: h.partner_is_great_power ? '#daa520' : '#999' }}>{h.partner_name}</span>
                            </span>
                          </td>
                          <td style={{ ...styles.td, color: '#daa520', textAlign: 'center' as const }}>
                            {h.partner_is_great_power ? '★' : ''}
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              ) : (
                <table style={styles.table}>
                  <thead>
                    <tr>
                      {HIST_COLS_AGG.map(col => (
                        <th
                          key={col.key}
                          style={{ ...styles.th, cursor: 'pointer', userSelect: 'none' as const }}
                          onClick={() => cycleAggSort(col.key)}
                        >
                          {col.label}{(() => { const s = aggSort.find(x => x.key === col.key); if (!s) return ''; const rank = aggSort.length > 1 ? String(aggSort.indexOf(s) + 1) : ''; return (s.dir === 1 ? ' ▲' : ' ▼') + rank; })()}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {aggregatedHistory.map((row, i) => (
                      <tr key={i}>
                        <td style={styles.td}>{row.turn}</td>
                        <td style={{ ...styles.td, color: '#daa520' }}>{resourceLabel(row.resource)}</td>
                        <td style={{ ...styles.td, fontSize: 11 }}>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                            {flagBySellerId[row.partner_id] && (
                              <Flag svg={flagBySellerId[row.partner_id]} width={18} height={12} title={row.partner_name} />
                            )}
                            <span style={{ color: row.partner_is_great_power ? '#daa520' : '#999' }}>
                              {row.partner_name}
                            </span>
                            {row.partner_is_great_power && <span style={{ color: '#daa520', marginLeft: 2 }}>★</span>}
                          </span>
                        </td>
                        <td style={{ ...styles.td, color: row.bought > 0 ? '#e63946' : '#555' }}>
                          {row.bought > 0 ? row.bought : '—'}
                        </td>
                        <td style={{ ...styles.td, color: row.bought > 0 ? '#e63946' : '#555' }}>
                          {row.bought > 0 ? `-$${row.boughtCost.toLocaleString()}` : '—'}
                        </td>
                        <td style={{ ...styles.td, color: row.sold > 0 ? '#2a9d8f' : '#555' }}>
                          {row.sold > 0 ? row.sold : '—'}
                        </td>
                        <td style={{ ...styles.td, color: row.sold > 0 ? '#2a9d8f' : '#555' }}>
                          {row.sold > 0 ? `+$${row.soldCost.toLocaleString()}` : '—'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        )}

        {activeTab === 'historical_market' && (
          <div style={styles.summaryBody}>
            {/* Turn sidebar */}
            <div style={styles.summarySidebar}>
              <div style={styles.summaryArchiveTitle}>Past Turns</div>
              {marketTurnsSorted.length === 0 && (
                <p style={{ color: '#666', fontSize: 12, padding: '8px 0' }}>No market data yet.</p>
              )}
              {marketTurnsSorted.map(t => {
                const rec = marketArchive.find((r: any) => r.turn === t);
                const count = rec?.offers?.length ?? 0;
                const active = (selectedMarketTurn ?? marketTurnsSorted[0]) === t;
                return (
                  <button
                    key={t}
                    style={active ? styles.summaryItemActive : styles.summaryItem}
                    onClick={() => setSelectedMarketTurn(t)}
                  >
                    Turn {t}
                    <span style={styles.summaryBadge}>{count}</span>
                  </button>
                );
              })}
            </div>

            {/* Market content */}
            <div style={styles.summaryContent}>
              {marketArchive.length > 0 && (
                <div style={{ ...styles.filterRow, marginBottom: 12 }}>
                  <FilterDropdown
                    values={marketCommodity}
                    options={marketCommodityOptions}
                    onChange={setMarketCommodity}
                    placeholder="All commodities"
                  />
                  <QuickChip label="Goods" groupValues={marketCommodityGroups.goods} values={marketCommodity} onChange={setMarketCommodity} />
                  <QuickChip label="Materials" groupValues={marketCommodityGroups.materials} values={marketCommodity} onChange={setMarketCommodity} />
                  <QuickChip label="Resources" groupValues={marketCommodityGroups.resources} values={marketCommodity} onChange={setMarketCommodity} />
                  <span style={styles.filterDivider} />
                  <FilterDropdown
                    values={marketCountry}
                    options={marketCountryOptions}
                    onChange={setMarketCountry}
                    placeholder="All countries"
                  />
                  <QuickChip label="Great Powers" groupValues={marketCountryGroups.great} values={marketCountry} onChange={setMarketCountry} />
                  <QuickChip label="Minor Powers" groupValues={marketCountryGroups.minor} values={marketCountry} onChange={setMarketCountry} />
                </div>
              )}
              {marketArchive.length === 0 ? (
                <p style={{ color: '#666' }}>No market activity recorded yet.</p>
              ) : sortedMarketOffers.length === 0 ? (
                <p style={{ color: '#666' }}>No offers match the filters.</p>
              ) : (
                <table style={styles.table}>
                  <thead>
                    <tr>
                      {MARKET_COLS.map(col => (
                        <th
                          key={col.key}
                          style={{ ...styles.th, cursor: 'pointer', userSelect: 'none' as const }}
                          onClick={() => cycleMarketSort(col.key)}
                        >
                          {col.label}{(() => { const s = marketSort.find(x => x.key === col.key); if (!s) return ''; const rank = marketSort.length > 1 ? String(marketSort.indexOf(s) + 1) : ''; return (s.dir === 1 ? ' ▲' : ' ▼') + rank; })()}
                        </th>
                      ))}
                      <th style={styles.th}>Bought by</th>
                    </tr>
                  </thead>
                  <tbody>
                    {sortedMarketOffers.map((row: any, i: number) => {
                      const sold = row.sold ?? 0;
                      const offered = row.offered ?? 0;
                      return (
                        <tr key={i}>
                          <td style={{ ...styles.td, color: '#daa520' }}>{resourceLabel(row.resource)}</td>
                          <td style={{ ...styles.td, fontSize: 11 }}>
                            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                              {flagBySellerId[row.seller_id] && (
                                <Flag svg={flagBySellerId[row.seller_id]} width={18} height={12} title={row.seller_name} />
                              )}
                              <span style={{ color: row.seller_is_great_power ? '#daa520' : '#999' }}>
                                {row.seller_name}
                              </span>
                              {row.seller_is_great_power && <span style={{ color: '#daa520', marginLeft: 2 }}>★</span>}
                            </span>
                          </td>
                          <td style={styles.td}>{offered}</td>
                          <td style={{ ...styles.td, color: sold === 0 ? '#666' : sold === offered ? '#2a9d8f' : '#daa520' }}>
                            {sold}
                          </td>
                          <td style={styles.td}>${row.price_per_unit}</td>
                          <td style={styles.td}>
                            {(row.fills ?? []).length === 0 ? (
                              <span style={{ color: '#555', fontSize: 11 }}>—</span>
                            ) : (
                              <div style={{ display: 'flex', flexWrap: 'wrap' as const, gap: 6, fontSize: 11 }}>
                                {row.fills.map((f: any, k: number) => (
                                  <span
                                    key={k}
                                    style={{ display: 'inline-flex', alignItems: 'center', gap: 3 }}
                                    title={`${f.buyer_name} bought ${f.quantity} @ $${f.price_per_unit}`}
                                  >
                                    {flagBySellerId[f.buyer_id] && (
                                      <Flag svg={flagBySellerId[f.buyer_id]} width={14} height={10} title={f.buyer_name} />
                                    )}
                                    <span style={{ color: f.buyer_is_great_power ? '#daa520' : '#999' }}>
                                      {f.buyer_name}
                                    </span>
                                    <span style={{ color: '#bbb' }}>×{f.quantity}</span>
                                  </span>
                                ))}
                              </div>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
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

/**
 * Cargo readout in the trade screen header (card #472). Shows used / total
 * tonnage next to a merchant-ship icon; hovering reveals a tooltip listing
 * each merchant-ship type with its count.
 */
function CargoSummary({ used, total, merchants }: { used: number; total: number; merchants: ShipDetail[] }) {
  const [open, setOpen] = useState(false);
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of merchants) m.set(s.type, (m.get(s.type) ?? 0) + 1);
    return Array.from(m.entries()).sort((a, b) => a[0].localeCompare(b[0]));
  }, [merchants]);

  return (
    <span
      style={{ position: 'relative', cursor: merchants.length > 0 ? 'help' : 'default', display: 'inline-flex', alignItems: 'center', gap: 4 }}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <span aria-hidden style={{ fontSize: 14 }}>{'⛵'}</span>
      <span>Cargo: {used} / {total}</span>
      {open && (
        <div
          role="tooltip"
          style={{
            position: 'absolute', top: '100%', left: 0, marginTop: 6,
            background: '#0f0f23', border: '1px solid #5a5030', borderRadius: 4,
            padding: '6px 10px', zIndex: 50, minWidth: 180, color: '#e0d8c0',
            fontSize: 12, lineHeight: 1.6, boxShadow: '0 4px 10px rgba(0,0,0,0.5)',
          }}
        >
          <div style={{ color: '#daa520', fontWeight: 'bold', marginBottom: 4 }}>
            Merchant fleet ({merchants.length})
          </div>
          {counts.length === 0 ? (
            <div style={{ color: '#888', fontStyle: 'italic' }}>No merchant ships</div>
          ) : counts.map(([type, count]) => (
            <div key={type} style={{ display: 'flex', justifyContent: 'space-between', gap: 16 }}>
              <span>{type.replace(/([A-Z])/g, ' $1').trim()}</span>
              <span style={{ color: '#aaa' }}>{count}</span>
            </div>
          ))}
        </div>
      )}
    </span>
  );
}

function partitionCommodities(opts: FilterOption[]): { goods: string[]; materials: string[]; resources: string[] } {
  const goods: string[] = [], materials: string[] = [], resources: string[] = [];
  for (const o of opts) {
    const c = commodityCategory(o.value);
    if (c === 'good') goods.push(o.value);
    else if (c === 'material') materials.push(o.value);
    else if (c === 'resource') resources.push(o.value);
  }
  return { goods, materials, resources };
}

function partitionCountries(opts: FilterOption[], gpIds: Set<number>): { great: string[]; minor: string[] } {
  const great: string[] = [], minor: string[] = [];
  for (const o of opts) {
    const id = Number(o.value);
    if (gpIds.has(id)) great.push(o.value);
    else minor.push(o.value);
  }
  return { great, minor };
}

interface QuickChipProps {
  label: string;
  groupValues: string[];
  values: string[];
  onChange: (v: string[]) => void;
}

function QuickChip({ label, groupValues, values, onChange }: QuickChipProps) {
  if (groupValues.length === 0) return null;
  const selectedSet = new Set(values);
  const allSelected = groupValues.every(v => selectedSet.has(v));
  const someSelected = !allSelected && groupValues.some(v => selectedSet.has(v));
  const active = allSelected;
  const toggle = () => {
    if (allSelected) {
      const remove = new Set(groupValues);
      onChange(values.filter(v => !remove.has(v)));
    } else {
      const merged = new Set(values);
      for (const v of groupValues) merged.add(v);
      onChange(Array.from(merged));
    }
  };
  return (
    <button
      type="button"
      onClick={toggle}
      title={`${label} (${groupValues.length})`}
      style={{
        padding: '3px 8px',
        background: active ? 'rgba(218,165,32,0.18)' : 'transparent',
        border: `1px solid ${active ? '#daa520' : someSelected ? '#7a5a20' : '#3a3520'}`,
        color: active ? '#daa520' : someSelected ? '#c0a050' : '#888',
        cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 11,
        borderRadius: 12, whiteSpace: 'nowrap',
      }}
    >
      {label}
      <span style={{ marginLeft: 4, color: active ? '#daa520' : '#666', fontSize: 10 }}>
        {groupValues.length}
      </span>
    </button>
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
  filterRow: {
    display: 'flex', alignItems: 'center', gap: 8,
    marginBottom: 8, flexWrap: 'wrap' as const,
  },
  filterDivider: {
    display: 'inline-block', width: 1, alignSelf: 'stretch',
    background: '#3a3520', margin: '0 4px',
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
