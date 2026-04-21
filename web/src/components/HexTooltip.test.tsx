import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { TileData, NavyMarker } from '../wasm';
import HexTooltip from './HexTooltip';

function makeTile(partial: Partial<TileData> = {}): TileData {
  return {
    q: 0, r: 0,
    terrain: 'Hills',
    resource: null, resource_hidden: false,
    is_capital: false, is_country_capital: false,
    improvement_level: 0, max_improvement_level: 0,
    owner: 'Devron', owner_color: 'Red', province: 'Bavaria',
    province_id: 1,
    has_railroad: false, has_depot: false, has_port: false,
    has_fort: false, fort_level: 0,
    map_width: 100, nation_id: 1,
    army_firepower: 0, army_unit_count: 0,
    army_composition: null,
    naval_firepower: 0, naval_ship_count: 0,
    civilian_on_tile: null,
    is_minor: false, is_incorporated_minor: false, is_anarchic: false,
    visual_group: null, visible: true,
    ...partial,
  };
}

function makeFleetMarker(): NavyMarker {
  return {
    q: 3, r: -1,
    nation_id: 2,
    owner_name: 'Kem',
    owner_color: 'Green',
    kind: 'fleet',
    ship_count: 4,
    total_fp: 42,
    total_hull: 120,
    by_type: { Frigate: 2, Ironclad: 2 },
    by_operation: { Patrol: 3, Escort: 1 },
    visible: true,
  };
}

describe('HexTooltip', () => {
  it('renders tile body with terrain, province, and owner', () => {
    render(
      <HexTooltip
        tile={makeTile({ terrain: 'Forest', province: 'Saxony', owner: 'Kem', has_railroad: true })}
        screenX={0}
        screenY={0}
        sticky={false}
      />,
    );
    expect(screen.getByText('Forest')).toBeInTheDocument();
    expect(screen.getByText(/Saxony/)).toBeInTheDocument();
    expect(screen.getByText(/Kem/)).toBeInTheDocument();
    expect(screen.getByText('Railroad')).toBeInTheDocument();
    // Non-sticky tooltip must not show the "Click to dismiss" hint.
    expect(screen.queryByText('Click to dismiss')).toBeNull();
  });

  it('renders resource + improvement level only when resource is present', () => {
    const tile = makeTile({ resource: 'Coal', improvement_level: 2, max_improvement_level: 3 });
    render(<HexTooltip tile={tile} screenX={0} screenY={0} sticky={false} />);
    expect(screen.getByText(/Coal/)).toBeInTheDocument();
    expect(screen.getByText('Level: 2/3')).toBeInTheDocument();
  });

  it('renders marker body with composition breakdown', () => {
    render(<HexTooltip marker={makeFleetMarker()} screenX={0} screenY={0} sticky={false} />);
    expect(screen.getByText(/Fleet/)).toBeInTheDocument();
    expect(screen.getByText(/Kem/)).toBeInTheDocument();
    expect(screen.getByText(/4 ships/)).toBeInTheDocument();
    expect(screen.getByText(/2 Frigate/)).toBeInTheDocument();
    expect(screen.getByText(/2 Ironclad/)).toBeInTheDocument();
    expect(screen.getByText(/3 Patrol/)).toBeInTheDocument();
  });

  it('shows the "Click to dismiss" hint when sticky', () => {
    render(<HexTooltip tile={makeTile()} screenX={0} screenY={0} sticky={true} />);
    expect(screen.getByText('Click to dismiss')).toBeInTheDocument();
  });

  it('shows civilian-on-tile details when present', () => {
    const tile = makeTile({
      terrain: 'Grassland',
      civilian_on_tile: {
        id: 42,
        type: 'Farmer',
        working: true,
        turns_remaining: 3,
        build_task: null,
        owner: 'Devron',
        owner_color: 'Red',
        is_human: true,
      },
    });
    render(<HexTooltip tile={tile} screenX={0} screenY={0} sticky={false} />);
    expect(screen.getByText(/Farmer/)).toBeInTheDocument();
    expect(screen.getByText(/working.*3t left/)).toBeInTheDocument();
  });

  it('shows army composition for capital tiles', () => {
    const tile = makeTile({
      is_capital: true,
      army_firepower: 42.4,
      army_unit_count: 5,
      army_composition: { Regulars: 3, Guards: 2 },
    });
    render(<HexTooltip tile={tile} screenX={0} screenY={0} sticky={false} />);
    expect(screen.getByText(/3 Regulars/)).toBeInTheDocument();
    expect(screen.getByText(/2 Guards/)).toBeInTheDocument();
    expect(screen.getByText(/42\.4 FP/)).toBeInTheDocument();
  });

  it('applies a yellow border only when sticky', () => {
    const { container, rerender } = render(
      <HexTooltip tile={makeTile()} screenX={0} screenY={0} sticky={false} />,
    );
    const nonStickyBorder = container.firstElementChild!.getAttribute('style') ?? '';
    expect(nonStickyBorder).toContain('rgb(90, 80, 48)'); // #5a5030

    rerender(<HexTooltip tile={makeTile()} screenX={0} screenY={0} sticky={true} />);
    const stickyBorder = container.firstElementChild!.getAttribute('style') ?? '';
    expect(stickyBorder).toContain('rgb(255, 217, 0)'); // #ffd900
  });
});
