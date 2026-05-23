import { useEffect, useRef, useState } from 'react';
import Flag from './Flag';

export interface FilterOption {
  value: string;
  label: string;
  emoji?: string;
  flagSvg?: string;
}

interface Props {
  values: string[];
  options: FilterOption[];
  onChange: (v: string[]) => void;
  placeholder: string;
  width?: number;
}

export default function FilterDropdown({ values, options, onChange, placeholder, width = 180 }: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open]);

  const selectedSet = new Set(values);
  const selectedOptions = options.filter(o => selectedSet.has(o.value));
  const active = values.length > 0;

  const renderIcon = (opt: FilterOption, size: 'sm' | 'md' = 'md') => {
    if (opt.flagSvg) {
      return <Flag svg={opt.flagSvg} width={size === 'sm' ? 16 : 18} height={size === 'sm' ? 11 : 12} title={opt.label} />;
    }
    if (opt.emoji) {
      return <span style={{ fontSize: size === 'sm' ? 12 : 14 }}>{opt.emoji}</span>;
    }
    return <span style={{ width: size === 'sm' ? 16 : 18 }} />;
  };

  const toggle = (value: string) => {
    if (selectedSet.has(value)) onChange(values.filter(v => v !== value));
    else onChange([...values, value]);
  };

  // Compact summary inside the button: show up to 3 icons + "+N" overflow.
  const MAX_ICONS = 3;
  const visibleIcons = selectedOptions.slice(0, MAX_ICONS);
  const overflow = selectedOptions.length - visibleIcons.length;

  return (
    <div ref={rootRef} style={{ position: 'relative', width }}>
      <button
        type="button"
        onClick={() => setOpen(o => !o)}
        style={{
          width: '100%',
          display: 'flex', alignItems: 'center', gap: 6,
          padding: '4px 8px',
          background: active ? 'rgba(218,165,32,0.12)' : '#0f0f23',
          border: `1px solid ${active ? '#daa520' : '#5a5030'}`,
          color: active ? '#daa520' : '#999',
          cursor: 'pointer',
          fontFamily: "'Georgia', serif",
          fontSize: 12,
          borderRadius: 3,
          textAlign: 'left',
        }}
      >
        {active ? (
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, flex: 1, minWidth: 0 }}>
            {visibleIcons.map(opt => (
              <span key={opt.value} style={{ display: 'inline-flex', alignItems: 'center' }} title={opt.label}>
                {renderIcon(opt, 'sm')}
              </span>
            ))}
            {overflow > 0 && <span style={{ color: '#daa520', fontSize: 11 }}>+{overflow}</span>}
            {selectedOptions.length === 1 && (
              <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', marginLeft: 2 }}>
                {selectedOptions[0].label}
              </span>
            )}
          </span>
        ) : (
          <span style={{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {placeholder}
          </span>
        )}
        {active && (
          <span
            onClick={e => { e.stopPropagation(); onChange([]); }}
            style={{ color: '#888', cursor: 'pointer', padding: '0 2px' }}
            title="Clear all"
          >×</span>
        )}
        <span style={{ color: '#888', fontSize: 9 }}>{open ? '▲' : '▼'}</span>
      </button>
      {open && (
        <div
          style={{
            position: 'absolute', top: '100%', left: 0, marginTop: 2,
            minWidth: '100%', maxHeight: 320, overflowY: 'auto',
            background: '#0f0f23', border: '1px solid #5a5030',
            borderRadius: 3, zIndex: 100,
            boxShadow: '0 4px 12px rgba(0,0,0,0.6)',
          }}
        >
          {options.length === 0 ? (
            <div style={{ padding: '8px 12px', color: '#666', fontSize: 11, fontStyle: 'italic' }}>
              No options
            </div>
          ) : (
            <>
              <div style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 8px', borderBottom: '1px solid #3a3520' }}>
                <button
                  type="button"
                  onClick={() => onChange(options.map(o => o.value))}
                  style={dropdownActionBtn}
                  disabled={values.length === options.length}
                >All</button>
                <button
                  type="button"
                  onClick={() => onChange([])}
                  style={dropdownActionBtn}
                  disabled={values.length === 0}
                >None</button>
              </div>
              {options.map(opt => {
                const checked = selectedSet.has(opt.value);
                return (
                  <button
                    key={opt.value}
                    type="button"
                    onClick={() => toggle(opt.value)}
                    style={{
                      width: '100%', display: 'flex', alignItems: 'center', gap: 6,
                      padding: '4px 8px',
                      background: checked ? 'rgba(218,165,32,0.12)' : 'transparent',
                      border: 'none',
                      color: checked ? '#daa520' : '#e0d8c0',
                      cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 12,
                      textAlign: 'left', whiteSpace: 'nowrap',
                    }}
                  >
                    <span
                      aria-hidden
                      style={{
                        display: 'inline-block', width: 12, height: 12,
                        border: `1px solid ${checked ? '#daa520' : '#5a5030'}`,
                        background: checked ? '#daa520' : 'transparent',
                        color: '#000', fontSize: 10, lineHeight: '10px', textAlign: 'center',
                        flexShrink: 0,
                      }}
                    >{checked ? '✓' : ''}</span>
                    {renderIcon(opt, 'sm')}
                    <span>{opt.label}</span>
                  </button>
                );
              })}
            </>
          )}
        </div>
      )}
    </div>
  );
}

const dropdownActionBtn: React.CSSProperties = {
  flex: 1, background: 'transparent', border: '1px solid #5a5030',
  color: '#999', cursor: 'pointer', fontFamily: "'Georgia', serif",
  fontSize: 10, padding: '2px 6px', borderRadius: 2, margin: '0 2px',
};
