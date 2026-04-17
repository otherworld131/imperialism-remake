import React, { useState } from 'react';
import type { TradeData, AvailableOffer } from '../wasm';

interface Props {
  trade: TradeData;
  onSetSubsidy: (targetNationId: number, amount: number) => void;
  onSetSellOrder: (commodityType: string, commodityName: string, quantity: number) => void;
  onSetBuyOrder: (resource: string, quantity: number, maxPrice: number) => void;
}

export default function TradePanel({ trade, onSetSubsidy, onSetSellOrder, onSetBuyOrder }: Props) {
  const {
    trade_history, subsidies, trade_balance, total_cargo,
    remaining_cargo, minor_nations, player_sell_orders, player_buy_orders,
    available_offers, sellable_resources, sellable_materials, sellable_goods,
  } = trade;

  const [expandedMN, setExpandedMN] = useState<number | null>(null);
  const [buyModalResource, setBuyModalResource] = useState<string | null>(null);

  const subsidyMap: Record<number, number> = {};
  for (const s of subsidies) subsidyMap[s.nation_id] = s.amount;

  // Aggregate available offers by resource for the buy section
  const offersByResource: Record<string, AvailableOffer[]> = {};
  for (const o of available_offers) {
    if (!offersByResource[o.resource]) offersByResource[o.resource] = [];
    offersByResource[o.resource].push(o);
  }

  // Current sell order quantities by commodity key
  const sellQtyMap: Record<string, number> = {};
  for (const o of player_sell_orders) sellQtyMap[`${o.commodity_type}:${o.commodity_name}`] = o.quantity;

  // Current buy order quantities by resource
  const buyQtyMap: Record<string, number> = {};
  for (const o of player_buy_orders) buyQtyMap[o.resource] = o.quantity;

  return (
    <div style={{ fontSize: 13 }}>
      {/* Cargo indicator */}
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
        <span style={{ fontWeight: 'bold', color: '#daa520' }}>Trade Orders</span>
        <span style={{ fontSize: 11, color: remaining_cargo > 0 ? '#aaa' : '#e66' }}>
          Cargo: {total_cargo - remaining_cargo} / {total_cargo}
        </span>
      </div>

      {/* ── SELL ORDERS ── */}
      <div style={{ marginBottom: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4, fontSize: 12 }}>Sell</div>

        {/* Resources */}
        {sellable_resources.length > 0 && (
          <SellSection
            label="Resources"
            items={sellable_resources}
            commodityType="resource"
            sellQtyMap={sellQtyMap}
            remainingCargo={remaining_cargo}
            onSetSellOrder={onSetSellOrder}
          />
        )}
        {/* Materials */}
        {sellable_materials.length > 0 && (
          <SellSection
            label="Materials"
            items={sellable_materials}
            commodityType="material"
            sellQtyMap={sellQtyMap}
            remainingCargo={remaining_cargo}
            onSetSellOrder={onSetSellOrder}
          />
        )}
        {/* Goods */}
        {sellable_goods.length > 0 && (
          <SellSection
            label="Goods"
            items={sellable_goods}
            commodityType="goods"
            sellQtyMap={sellQtyMap}
            remainingCargo={remaining_cargo}
            onSetSellOrder={onSetSellOrder}
          />
        )}
        {sellable_resources.length === 0 && sellable_materials.length === 0 && sellable_goods.length === 0 && (
          <div style={{ color: '#888', fontStyle: 'italic', fontSize: 11 }}>No commodities to sell</div>
        )}
      </div>

      {/* ── BUY ORDERS ── */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginBottom: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4, fontSize: 12 }}>Buy</div>
        {Object.keys(offersByResource).length === 0 ? (
          <div style={{ color: '#888', fontStyle: 'italic', fontSize: 11 }}>No resources available from minor nations</div>
        ) : (
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 36px 36px auto', gap: '1px 4px', fontSize: 11 }}>
            <span style={colHeaderStyle}>Resource</span>
            <span style={colHeaderStyle}>Avail</span>
            <span style={colHeaderStyle}>Price</span>
            <span style={colHeaderStyle}></span>
            {Object.entries(offersByResource).map(([resource, offers]) => {
              const totalAvail = offers.reduce((s, o) => s + o.quantity, 0);
              const avgPrice = Math.round(offers.reduce((s, o) => s + o.price * o.quantity, 0) / totalAvail);
              const currentQty = buyQtyMap[resource] ?? 0;
              return (
                <React.Fragment key={resource}>
                  <span>{resource}</span>
                  <span style={{ color: '#aaa' }}>{totalAvail}</span>
                  <span style={{ color: '#daa520' }}>${avgPrice}</span>
                  <button
                    onClick={() => setBuyModalResource(resource)}
                    style={{
                      ...smallBtn,
                      background: currentQty > 0 ? '#daa520' : '#3a3520',
                      color: currentQty > 0 ? '#000' : '#e0d8c0',
                    }}
                  >
                    {currentQty > 0 ? `${currentQty} ordered` : 'Buy'}
                  </button>
                </React.Fragment>
              );
            })}
          </div>
        )}
      </div>

      {/* ── TRADE PARTNERS (subsidies) ── */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginBottom: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Trade Partners</div>
        {minor_nations.length === 0 && (
          <div style={{ color: '#888', fontStyle: 'italic', fontSize: 12, marginBottom: 6 }}>
            No minor nations found
          </div>
        )}
        {minor_nations.map(mn => {
          const subsidy = subsidyMap[mn.nation_id] ?? 0;
          const isExpanded = expandedMN === mn.nation_id;
          return (
            <div key={mn.nation_id} style={{
              background: 'rgba(255,255,255,0.03)', borderRadius: 3,
              padding: '3px 5px', marginBottom: 3,
            }}>
              <div
                style={{ display: 'flex', justifyContent: 'space-between', cursor: 'pointer', alignItems: 'center' }}
                onClick={() => setExpandedMN(isExpanded ? null : mn.nation_id)}
              >
                <span style={{ fontSize: 12 }}>
                  {mn.name}
                  {mn.has_consulate && <span style={{ fontSize: 9, color: '#2a6', marginLeft: 4 }} title="Consulate: trade improves relations">&#9733;</span>}
                </span>
                <span style={{ fontSize: 10, color: '#999' }}>
                  {mn.resources.join(', ')}
                </span>
              </div>
              {isExpanded && (
                <div style={{ marginTop: 4, display: 'flex', alignItems: 'center', gap: 4 }}>
                  <span style={{ fontSize: 11, color: '#aaa' }}>Subsidy:</span>
                  {[0, 500, 1000, 2000].map(amt => (
                    <button
                      key={amt}
                      onClick={() => onSetSubsidy(mn.nation_id, amt)}
                      style={{
                        ...smallBtn,
                        background: subsidy === amt ? '#daa520' : '#3a3520',
                        color: subsidy === amt ? '#000' : '#e0d8c0',
                      }}
                    >
                      ${amt}
                    </button>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* ── TRADE BALANCE ── */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginBottom: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Trade Balance</div>
        <div style={{ fontSize: 11, display: 'flex', justifyContent: 'space-between' }}>
          <span>Imports: <span style={{ color: '#e66' }}>${trade_balance.total_bought}</span></span>
          <span>Exports: <span style={{ color: '#6e6' }}>${trade_balance.total_sold}</span></span>
        </div>
        <div style={{ fontSize: 11, marginTop: 2 }}>
          Net: <span style={{ color: trade_balance.net >= 0 ? '#6e6' : '#e66' }}>${trade_balance.net}</span>
        </div>
      </div>

      {/* ── TRADE HISTORY ── */}
      {trade_history.length > 0 && (
        <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8 }}>
          <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Recent Trades</div>
          <div style={{ maxHeight: 100, overflowY: 'auto' }}>
            {trade_history.map((h, i) => (
              <div key={i} style={{ fontSize: 10, color: '#aaa', marginBottom: 2 }}>
                T{h.turn}: {h.bought ? 'bought' : 'sold'} {h.quantity} {h.resource} {h.bought ? 'from' : 'to'} {h.partner_name} (${h.total_cost})
              </div>
            ))}
          </div>
        </div>
      )}

      {/* ── BUY MODAL ── */}
      {buyModalResource && (
        <BuyModal
          resource={buyModalResource}
          offers={offersByResource[buyModalResource] ?? []}
          currentQty={buyQtyMap[buyModalResource] ?? 0}
          remainingCargo={remaining_cargo + (buyQtyMap[buyModalResource] ?? 0)}
          onConfirm={(qty, maxPrice) => {
            onSetBuyOrder(buyModalResource, qty, maxPrice);
            setBuyModalResource(null);
          }}
          onCancel={() => setBuyModalResource(null)}
        />
      )}
    </div>
  );
}

// ── SellSection component ──

function SellSection({ label, items, commodityType, sellQtyMap, remainingCargo, onSetSellOrder }: {
  label: string;
  items: { name: string; stock: number; price: number }[];
  commodityType: string;
  sellQtyMap: Record<string, number>;
  remainingCargo: number;
  onSetSellOrder: (commodityType: string, commodityName: string, quantity: number) => void;
}) {
  return (
    <div style={{ marginBottom: 4 }}>
      <div style={{ fontSize: 10, color: '#888', textTransform: 'uppercase', marginBottom: 2 }}>{label}</div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 30px 36px 60px', gap: '1px 4px', fontSize: 11 }}>
        {items.map(item => {
          const key = `${commodityType}:${item.name}`;
          const currentQty = sellQtyMap[key] ?? 0;
          return (
            <React.Fragment key={item.name}>
              <span>{item.name}</span>
              <span style={{ color: '#aaa' }}>{item.stock}</span>
              <span style={{ color: '#daa520' }}>${item.price}</span>
              <span style={{ display: 'flex', gap: 2, alignItems: 'center' }}>
                <button
                  style={tinyBtn}
                  onClick={() => onSetSellOrder(commodityType, item.name, Math.max(0, currentQty - 1))}
                  disabled={currentQty === 0}
                >-</button>
                <span style={{ minWidth: 14, textAlign: 'center', color: currentQty > 0 ? '#daa520' : '#888' }}>
                  {currentQty}
                </span>
                <button
                  style={tinyBtn}
                  onClick={() => onSetSellOrder(commodityType, item.name, currentQty + 1)}
                  disabled={currentQty >= item.stock || remainingCargo <= 0}
                >+</button>
              </span>
            </React.Fragment>
          );
        })}
      </div>
    </div>
  );
}

// ── BuyModal component ──

function BuyModal({ resource, offers, currentQty, remainingCargo, onConfirm, onCancel }: {
  resource: string;
  offers: AvailableOffer[];
  currentQty: number;
  remainingCargo: number;
  onConfirm: (quantity: number, maxPrice: number) => void;
  onCancel: () => void;
}) {
  const [quantity, setQuantity] = useState(currentQty);
  const totalAvail = offers.reduce((s, o) => s + o.quantity, 0);
  const maxQty = Math.min(totalAvail, remainingCargo);
  const avgPrice = totalAvail > 0
    ? Math.round(offers.reduce((s, o) => s + o.price * o.quantity, 0) / totalAvail)
    : 0;
  // Max price at 120% to ensure we outbid competitors
  const maxPrice = Math.round(avgPrice * 1.2);

  return (
    <div style={backdropStyle} onClick={onCancel}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <div style={modalHeaderStyle}>
          <h3 style={{ margin: 0, color: '#daa520', fontSize: 16 }}>Buy {resource}</h3>
        </div>
        <div style={{ padding: '12px 16px' }}>
          {/* Source nations */}
          <div style={{ fontSize: 12, marginBottom: 10 }}>
            <div style={{ color: '#888', fontSize: 10, textTransform: 'uppercase', marginBottom: 4 }}>Sources</div>
            {offers.map((o, i) => (
              <div key={i} style={{
                display: 'flex', justifyContent: 'space-between',
                padding: '3px 6px', background: 'rgba(255,255,255,0.03)',
                borderRadius: 3, marginBottom: 2,
              }}>
                <span>{o.seller_name}</span>
                <span>
                  <span style={{ color: '#aaa' }}>{o.quantity} units</span>
                  <span style={{ color: '#daa520', marginLeft: 8 }}>${o.price}/ea</span>
                </span>
              </div>
            ))}
          </div>

          {/* Quantity selector */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
            <span style={{ fontSize: 12 }}>Quantity:</span>
            <button style={qtyBtn} onClick={() => setQuantity(Math.max(0, quantity - 1))} disabled={quantity === 0}>-</button>
            <span style={{ minWidth: 24, textAlign: 'center', fontSize: 14, fontWeight: 'bold', color: '#daa520' }}>
              {quantity}
            </span>
            <button style={qtyBtn} onClick={() => setQuantity(Math.min(maxQty, quantity + 1))} disabled={quantity >= maxQty}>+</button>
            <span style={{ fontSize: 10, color: '#888' }}>/ {maxQty} max</span>
          </div>

          {/* Cost summary */}
          <div style={{ fontSize: 12, color: '#aaa', marginBottom: 10 }}>
            Est. cost: <span style={{ color: '#e66' }}>${quantity * avgPrice}</span>
            <span style={{ color: '#666', marginLeft: 6 }}>(max bid ${maxPrice}/ea)</span>
          </div>
        </div>

        <div style={{ padding: '8px 16px', borderTop: '1px solid #3a3520', display: 'flex', justifyContent: 'flex-end', gap: 6 }}>
          <button onClick={onCancel} style={cancelBtnStyle}>Cancel</button>
          <button
            onClick={() => onConfirm(quantity, maxPrice)}
            style={confirmBtnStyle}
          >
            {quantity > 0 ? `Buy ${quantity} for ~$${quantity * avgPrice}` : 'Clear order'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Styles ──

const colHeaderStyle: React.CSSProperties = {
  color: '#888', fontSize: 10, textTransform: 'uppercase', borderBottom: '1px solid #3a3520',
  paddingBottom: 2, marginBottom: 2,
};

const smallBtn: React.CSSProperties = {
  border: 'none', borderRadius: 2, padding: '1px 6px', fontSize: 10, cursor: 'pointer',
};

const tinyBtn: React.CSSProperties = {
  border: 'none', borderRadius: 2, padding: '0 4px', fontSize: 11, cursor: 'pointer',
  background: '#3a3520', color: '#e0d8c0', lineHeight: '16px',
};

const backdropStyle: React.CSSProperties = {
  position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)',
  display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000,
};

const modalStyle: React.CSSProperties = {
  background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 8,
  maxWidth: 420, width: '90%', color: '#e0d8c0',
};

const modalHeaderStyle: React.CSSProperties = {
  padding: '12px 16px', borderBottom: '1px solid #3a3520',
};

const qtyBtn: React.CSSProperties = {
  border: 'none', borderRadius: 3, padding: '2px 8px', fontSize: 14, cursor: 'pointer',
  background: '#3a3520', color: '#e0d8c0',
};

const cancelBtnStyle: React.CSSProperties = {
  background: '#3a3520', color: '#e0d8c0', border: 'none', borderRadius: 3,
  padding: '4px 14px', fontSize: 12, cursor: 'pointer',
};

const confirmBtnStyle: React.CSSProperties = {
  background: '#2a6', color: '#fff', border: 'none', borderRadius: 3,
  padding: '4px 14px', fontSize: 12, cursor: 'pointer',
};
