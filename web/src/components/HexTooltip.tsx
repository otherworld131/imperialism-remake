import type { ReactNode } from 'react';
import type { TileData, NavyMarker } from '../wasm';
import { resourceLabel } from '../resourceEmoji';

interface Props {
  /** Tile under the cursor (mutually exclusive with `marker`). */
  tile?: TileData;
  /** Navy marker under the cursor (mutually exclusive with `tile`). */
  marker?: NavyMarker;
  /** Client-space position inside the map wrapper where the cursor is. */
  screenX: number;
  screenY: number;
  /** When true, the tooltip is pinned and absorbs pointer events. */
  sticky: boolean;
  /** Whether to surface hidden resources in the tile line. */
  showHiddenResources?: boolean;
  /** Optional lookup of nation_id → full government title (e.g., "Kingdom of Pram"). */
  governmentTitleByNationId?: Record<number, string>;
  /**
   * Slot for mode-specific content (diplomatic / military / naval strips)
   * the parent wants to show for a tile. Rendered under the generic tile info.
   */
  modeExtras?: ReactNode;
}

const BASE_STYLE: React.CSSProperties = {
  position: 'absolute',
  background: 'rgba(31, 27, 16, 0.96)',
  color: '#e0d8c0',
  borderRadius: 4,
  padding: '8px 10px',
  fontSize: 12,
  fontFamily: 'Georgia, serif',
  lineHeight: 1.4,
  maxWidth: 280,
  zIndex: 20,
  boxShadow: '0 4px 12px rgba(0, 0, 0, 0.4)',
};

export default function HexTooltip({
  tile, marker, screenX, screenY, sticky, showHiddenResources = false,
  governmentTitleByNationId, modeExtras,
}: Props) {
  if (!tile && !marker) return null;

  const style: React.CSSProperties = {
    ...BASE_STYLE,
    left: screenX + 14,
    top: screenY + 14,
    border: `1px solid ${sticky ? '#ffd900' : '#5a5030'}`,
    pointerEvents: sticky ? 'auto' : 'none',
  };

  return (
    <div style={style} role="tooltip">
      {tile && (
        <>
          <div style={{ marginBottom: 4 }}>
            <b>
              {tile.terrain}
              {tile.resource && (!tile.resource_hidden || showHiddenResources)
                ? ` \u2014 ${resourceLabel(tile.resource)}`
                : ''}
            </b>
          </div>
          {tile.province && (() => {
            // For incorporated provinces, look up the original minor — the
            // title still reads "Province of <Minor>" even though the GP
            // owns it. Falls back to the owner's title for normal tiles.
            const displayNid = tile.incorporated_nation_id ?? tile.nation_id;
            const ownerTitle = governmentTitleByNationId?.[displayNid] || tile.owner;
            return (
              <div>
                Province: {tile.province}
                {ownerTitle ? `, Province of ${ownerTitle}` : ''}
              </div>
            );
          })()}
          {!tile.province && tile.owner && <div>Owner: {tile.owner}</div>}
          {tile.resource && (!tile.resource_hidden || showHiddenResources) && (
            <div>Level: {tile.improvement_level}/{tile.max_improvement_level}</div>
          )}
          {tile.is_capital && <div>{'\u2605'} Capital</div>}
          {tile.has_railroad && <div>Railroad</div>}
          {tile.has_port && <div>Port</div>}
          {tile.has_depot && <div>Depot</div>}
          {tile.has_fort && <div>Fort L{tile.fort_level}</div>}
          {tile.civilian_on_tile && (
            <div style={{ marginTop: 4, fontSize: 11, color: '#bbb' }}>
              {tile.civilian_on_tile.type}
              {tile.civilian_on_tile.working ? ' (working' : ' (idle'}
              {tile.civilian_on_tile.working && tile.civilian_on_tile.turns_remaining > 0
                ? `, ${tile.civilian_on_tile.turns_remaining}t left`
                : ''}
              {tile.civilian_on_tile.build_task ? `, building ${tile.civilian_on_tile.build_task}` : ''}
              {')'}
              {tile.civilian_on_tile.owner && tile.civilian_on_tile.owner !== tile.owner
                ? ` \u2014 ${tile.civilian_on_tile.owner}`
                : ''}
            </div>
          )}
          {tile.army_composition && Object.keys(tile.army_composition).length > 0 && (
            <div style={{ marginTop: 4, fontSize: 11, color: '#bbb' }}>
              Army: {Object.entries(tile.army_composition).map(([t, n]) => `${n} ${t}`).join(', ')}
              {tile.army_firepower > 0 && ` \u00b7 ${tile.army_firepower.toFixed(1)} FP`}
            </div>
          )}
          {modeExtras}
        </>
      )}
      {marker && (
        <>
          <div style={{ marginBottom: 4, color: marker.kind === 'beachhead' ? '#ff8059' : '#e0d8c0' }}>
            <b>
              {marker.kind === 'beachhead'
                ? `Beachhead \u2192 ${marker.target_province ?? '?'}`
                : `Fleet \u2014 ${marker.owner_name}`}
            </b>
          </div>
          <div style={{ fontSize: 11, color: '#bbb' }}>
            {marker.ship_count} ships &middot; {marker.total_fp} FP &middot; {marker.total_hull} hull
          </div>
          {Object.keys(marker.by_type).length > 0 && (
            <div style={{ fontSize: 11, color: '#bbb', marginTop: 4 }}>
              {Object.entries(marker.by_type).map(([t, n]) => `${n} ${t}`).join(', ')}
            </div>
          )}
          {Object.keys(marker.by_operation).length > 0 && (
            <div style={{ fontSize: 11, color: '#888', marginTop: 2 }}>
              {Object.entries(marker.by_operation).map(([op, n]) => `${n} ${op}`).join(' \u00b7 ')}
            </div>
          )}
        </>
      )}
      {sticky && (
        <div style={{ fontSize: 10, color: '#888', marginTop: 6, fontStyle: 'italic' }}>
          Click to dismiss
        </div>
      )}
    </div>
  );
}
