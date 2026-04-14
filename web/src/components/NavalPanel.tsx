import type { ShipsData, BuildableUnit } from '../wasm';

interface Props {
  ships: ShipsData;
  buildableShips: BuildableUnit[];
  onBuildShip: (shipType: string) => void;
}

export default function NavalPanel({ ships, buildableShips, onBuildShip }: Props) {
  const { merchants, warships, total_cargo, total_naval_fp } = ships;

  return (
    <div style={{ fontSize: 13 }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6, color: '#ccc' }}>Naval Forces</div>

      {/* Merchant fleet */}
      <div style={{ marginBottom: 8 }}>
        <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>
          Merchants ({merchants.length} ships, {total_cargo} cargo)
        </div>
        {merchants.length > 0 ? merchants.map(s => (
          <div key={s.id} style={{
            display: 'flex', justifyContent: 'space-between',
            padding: '2px 4px', background: 'rgba(255,255,255,0.05)', borderRadius: 3, marginBottom: 2,
            fontSize: 12,
          }}>
            <span>\u2693 {formatShipType(s.type)}</span>
            <span style={{ color: '#888' }}>
              Hull {s.hull}/{s.hull_max} | Cargo {s.cargo}
            </span>
          </div>
        )) : (
          <div style={{ color: '#666', fontStyle: 'italic', fontSize: 11 }}>No merchant ships</div>
        )}
      </div>

      {/* Warship fleet */}
      <div style={{ marginBottom: 8 }}>
        <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>
          Warships ({warships.length} ships, {total_naval_fp} FP)
        </div>
        {warships.length > 0 ? warships.map(s => (
          <div key={s.id} style={{
            display: 'flex', justifyContent: 'space-between',
            padding: '2px 4px', background: 'rgba(255,255,255,0.05)', borderRadius: 3, marginBottom: 2,
            fontSize: 12,
          }}>
            <span>\u2693 {formatShipType(s.type)}</span>
            <span style={{ color: '#888' }}>
              Hull {s.hull}/{s.hull_max} | FP {s.firepower}
            </span>
          </div>
        )) : (
          <div style={{ color: '#666', fontStyle: 'italic', fontSize: 11 }}>No warships</div>
        )}
      </div>

      {/* Build section */}
      <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8 }}>
        <div style={{ fontWeight: 'bold', marginBottom: 4, color: '#ccc' }}>Build Ship</div>

        {/* Merchant ships */}
        <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>Merchants</div>
        {buildableShips.filter(b => b.category === 'Merchant').map(b => (
          <ShipBuildRow key={b.type} buildable={b} onBuild={onBuildShip} />
        ))}

        {/* Warships */}
        <div style={{ fontSize: 11, color: '#888', marginBottom: 3, marginTop: 6 }}>Warships</div>
        {buildableShips.filter(b => b.category === 'Warship').map(b => (
          <ShipBuildRow key={b.type} buildable={b} onBuild={onBuildShip} />
        ))}
      </div>
    </div>
  );
}

function ShipBuildRow({ buildable: b, onBuild }: { buildable: BuildableUnit; onBuild: (t: string) => void }) {
  const canBuild = b.can_afford && b.tech_met;
  const costs = b.resources_needed
    ? Object.entries(b.resources_needed).map(([k, v]) => `${v} ${k.toLowerCase()}`).join(', ')
    : '';

  return (
    <div style={{
      display: 'flex', justifyContent: 'space-between', alignItems: 'center',
      padding: '2px 0', opacity: canBuild ? 1 : 0.45,
    }}>
      <span style={{ fontSize: 12 }}>
        {formatShipType(b.type)}
        <span style={{ color: '#888', fontSize: 10, marginLeft: 4 }}>{costs}</span>
      </span>
      {canBuild ? (
        <button
          onClick={() => onBuild(b.type)}
          style={{ background: '#246', color: '#fff', border: 'none', borderRadius: 3, padding: '1px 6px', fontSize: 10, cursor: 'pointer' }}
        >
          Build
        </button>
      ) : (
        <span style={{ fontSize: 10, color: '#a66' }}>{b.reason || 'Unavailable'}</span>
      )}
    </div>
  );
}

function formatShipType(t: string): string {
  return t.replace(/([A-Z])/g, ' $1').trim();
}
