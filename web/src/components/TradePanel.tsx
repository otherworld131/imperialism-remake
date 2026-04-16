import React, { useState } from 'react';
import type { TradeData } from '../wasm';

interface Props {
  trade: TradeData;
  onSetSubsidy: (targetNationId: number, amount: number) => void;
}

export default function TradePanel({ trade, onSetSubsidy }: Props) {
  const { market_prices, trade_history, subsidies, trade_balance, total_cargo, minor_nations } = trade;
  const [expandedMN, setExpandedMN] = useState<number | null>(null);

  const subsidyMap: Record<number, number> = {};
  for (const s of subsidies) subsidyMap[s.nation_id] = s.amount;

  return (
    <div style={{ fontSize: 13 }}>
      {/* Trade Partners */}
      <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Trade Partners</div>
      {minor_nations.filter(mn => mn.has_consulate).length === 0 && (
        <div style={{ color: '#888', fontStyle: 'italic', fontSize: 12, marginBottom: 6 }}>
          Build consulates to open trade routes
        </div>
      )}
      {minor_nations.filter(mn => mn.has_consulate).map(mn => {
        const subsidy = subsidyMap[mn.nation_id] ?? 0;
        const isExpanded = expandedMN === mn.nation_id;
        return (
          <div key={mn.nation_id} style={{
            background: 'rgba(255,255,255,0.03)', borderRadius: 3,
            padding: '3px 5px', marginBottom: 3,
          }}>
            <div
              style={{ display: 'flex', justifyContent: 'space-between', cursor: 'pointer' }}
              onClick={() => setExpandedMN(isExpanded ? null : mn.nation_id)}
            >
              <span style={{ fontSize: 12 }}>{mn.name}</span>
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

      {/* Market Prices */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Market Prices</div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 40px 30px', gap: '1px 6px', fontSize: 11 }}>
          <span style={headerStyle}>Resource</span>
          <span style={headerStyle}>Price</span>
          <span style={headerStyle}>Stock</span>
          {market_prices.map(mp => (
            <React.Fragment key={mp.resource}>
              <span>{mp.resource}</span>
              <span style={{ color: '#daa520' }}>${mp.base_price}</span>
              <span style={{ color: '#aaa' }}>{mp.stock}</span>
            </React.Fragment>
          ))}
        </div>
      </div>

      {/* Trade Balance */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Trade Balance</div>
        <div style={{ fontSize: 11, display: 'flex', justifyContent: 'space-between' }}>
          <span>Imports: <span style={{ color: '#e66' }}>${trade_balance.total_bought}</span></span>
          <span>Exports: <span style={{ color: '#6e6' }}>${trade_balance.total_sold}</span></span>
        </div>
        <div style={{ fontSize: 11, marginTop: 2 }}>
          Net: <span style={{ color: trade_balance.net >= 0 ? '#6e6' : '#e66' }}>${trade_balance.net}</span>
          <span style={{ color: '#888', marginLeft: 8 }}>Cargo: {total_cargo}</span>
        </div>
      </div>

      {/* Trade History */}
      {trade_history.length > 0 && (
        <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 8 }}>
          <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Recent Trades</div>
          <div style={{ maxHeight: 120, overflowY: 'auto' }}>
            {trade_history.map((h, i) => (
              <div key={i} style={{ fontSize: 10, color: '#aaa', marginBottom: 2 }}>
                T{h.turn}: {h.quantity} {h.resource} from {h.partner_name} (${h.total_cost})
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

const headerStyle: React.CSSProperties = {
  color: '#888', fontSize: 10, textTransform: 'uppercase', borderBottom: '1px solid #3a3520',
  paddingBottom: 2, marginBottom: 2,
};

const smallBtn: React.CSSProperties = {
  border: 'none', borderRadius: 2, padding: '1px 4px', fontSize: 10, cursor: 'pointer',
};
