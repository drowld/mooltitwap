// petal-app.jsx — Petal v2 multitap delay mockup.
// Two visualizer views: Stems (read-only overview) and Grid (editable time × volume balls).

const { useState, useMemo, useRef, useEffect } = React;
const { useTweaks, TweaksPanel, TweakSection, TweakToggle, TweakRadio, TweakColor, TweakSlider } = window;

// ─── Default tweak state ──────────────────────────────────────────────────
const TWEAK_DEFAULTS = {
  "accent": "#a06cf0",
  "vizMode": "stems",
  "showGrid": true,
  "softUI": false
};

const ACCENT_OPTS = [
  '#a06cf0', // violet (default)
  '#c96442', // orange-rose (Petal classic)
  '#5b8a72', // botanical green
  '#5a9be8', // arctic blue
  '#e85a8a', // hot pink
];

// ─── Tap data ──────────────────────────────────────────────────────────────
// gain is 0..1, time is 0..1 (along the timeline), pitch in semitones
const INITIAL_TAPS = [
  { on: true,  pitch:  0, pan: -0.20, time: 0.06, gain: 0.85, lp: 0.85 },
  { on: true,  pitch:  5, pan:  0.30, time: 0.14, gain: 0.78, lp: 0.72 },
  { on: true,  pitch:  0, pan: -0.40, time: 0.23, gain: 0.62, lp: 0.65 },
  { on: false, pitch:  7, pan:  0.00, time: 0.34, gain: 0.55, lp: 0.55 },
  { on: true,  pitch: -5, pan:  0.50, time: 0.46, gain: 0.55, lp: 0.48 },
  { on: true,  pitch:  0, pan: -0.30, time: 0.58, gain: 0.42, lp: 0.42 },
  { on: false, pitch: 12, pan:  0.00, time: 0.72, gain: 0.32, lp: 0.38 },
  { on: true,  pitch: -7, pan:  0.40, time: 0.88, gain: 0.30, lp: 0.32 },
];

// ─── Inline SVG glyphs ─────────────────────────────────────────────────────
const Icon = ({ name, size = 14, stroke = 'currentColor', sw = 1.4 }) => {
  const common = { width: size, height: size, viewBox: '0 0 24 24', fill: 'none',
                   stroke, strokeWidth: sw, strokeLinecap: 'round', strokeLinejoin: 'round' };
  switch (name) {
    case 'logo':
      return (
        <svg width={size} height={size} viewBox="0 0 24 24">
          <g fill={stroke} fillOpacity="0.85">
            <ellipse cx="12" cy="12" rx="9" ry="3.4" transform="rotate(-30 12 12)"/>
            <ellipse cx="12" cy="12" rx="9" ry="3.4" transform="rotate(30 12 12)"/>
            <ellipse cx="12" cy="12" rx="9" ry="3.4" transform="rotate(90 12 12)"/>
          </g>
          <circle cx="12" cy="12" r="1.6" fill="#fff"/>
        </svg>
      );
    case 'chev-l': return <svg {...common}><path d="M15 6l-6 6 6 6"/></svg>;
    case 'chev-r': return <svg {...common}><path d="M9 6l6 6-6 6"/></svg>;
    case 'help':   return <svg {...common}><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 015 .5c0 1.5-2.5 2-2.5 4"/><circle cx="12" cy="17" r="0.5" fill={stroke}/></svg>;
    case 'undo':   return <svg {...common}><path d="M3 7v6h6"/><path d="M3 13a9 9 0 109-9"/></svg>;
    case 'redo':   return <svg {...common}><path d="M21 7v6h-6"/><path d="M21 13a9 9 0 11-9-9"/></svg>;
    case 'save':   return <svg {...common}><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M7 3v6h10V3"/><path d="M7 21v-8h10v8"/></svg>;
    case 'dice':   return <svg {...common}><rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8" cy="8" r="1" fill={stroke}/><circle cx="16" cy="16" r="1" fill={stroke}/><circle cx="16" cy="8" r="1" fill={stroke}/><circle cx="8" cy="16" r="1" fill={stroke}/><circle cx="12" cy="12" r="1" fill={stroke}/></svg>;
    case 'init':   return <svg {...common}><path d="M21 12a9 9 0 11-3-6.7"/><path d="M21 3v6h-6"/></svg>;
    case 'link':   return <svg {...common}><path d="M10 14a4 4 0 005.66 0l3-3a4 4 0 00-5.66-5.66l-1 1"/><path d="M14 10a4 4 0 00-5.66 0l-3 3a4 4 0 005.66 5.66l1-1"/></svg>;
    case 'stems':
      return <svg width={size} height={size} viewBox="0 0 24 24"><g fill={stroke}><rect x="3"  y="6"  width="2" height="6" rx="1"/><rect x="7"  y="4"  width="2" height="8" rx="1"/><rect x="11" y="7"  width="2" height="5" rx="1"/><rect x="15" y="3"  width="2" height="9" rx="1"/><rect x="19" y="6"  width="2" height="6" rx="1"/><rect x="3"  y="12" width="2" height="4" rx="1" opacity=".5"/><rect x="7"  y="12" width="2" height="6" rx="1" opacity=".5"/><rect x="11" y="12" width="2" height="3" rx="1" opacity=".5"/><rect x="15" y="12" width="2" height="7" rx="1" opacity=".5"/><rect x="19" y="12" width="2" height="5" rx="1" opacity=".5"/></g></svg>;
    case 'grid':
      // dots scattered on a grid — represents the "balls on grid" view
      return <svg width={size} height={size} viewBox="0 0 24 24"><g fill={stroke}>
        <circle cx="5"  cy="14" r="1.8"/><circle cx="9"  cy="9"  r="1.8"/>
        <circle cx="14" cy="15" r="1.8"/><circle cx="19" cy="7"  r="1.8"/>
      </g><g stroke={stroke} strokeWidth="0.5" opacity=".3">
        <line x1="2" y1="20" x2="22" y2="20"/><line x1="2" y1="4" x2="2" y2="20"/>
      </g></svg>;

    // ─── Right panel tab icons ─────────────────────────────
    case 'tab-pitch':
      // half-step / piano-key motif
      return <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
        <path d="M4 18 L4 12 L10 12 L10 6 L20 6" stroke={stroke} strokeWidth="1.6"
              strokeLinecap="round" strokeLinejoin="round"/>
        <circle cx="20" cy="6" r="1.5" fill={stroke}/>
        <circle cx="4" cy="18" r="1.5" fill={stroke}/>
      </svg>;
    case 'tab-pan':
      // L/R speakers with dot in middle
      return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={stroke}
                  strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <path d="M4 12 H8 L11 8 V16 L8 12"/>
        <path d="M20 12 H16 L13 8 V16 L16 12"/>
        <circle cx="12" cy="12" r="0.8" fill={stroke}/>
      </svg>;
    case 'tab-gain':
      // ascending bars
      return <svg width={size} height={size} viewBox="0 0 24 24" fill={stroke}>
        <rect x="3"  y="14" width="3" height="6" rx="0.5"/>
        <rect x="8"  y="10" width="3" height="10" rx="0.5"/>
        <rect x="13" y="6"  width="3" height="14" rx="0.5"/>
        <rect x="18" y="3"  width="3" height="17" rx="0.5"/>
      </svg>;
    case 'tab-filter':
      // filter curve (lowpass)
      return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={stroke}
                  strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
        <path d="M3 12 H10 Q13 12 14 8 Q15 18 17 18 L21 18"/>
      </svg>;
    case 'tab-xfeed':
      // crossing arrows
      return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={stroke}
                  strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <path d="M4 7 L14 7 M14 7 L11 4 M14 7 L11 10"/>
        <path d="M20 17 L10 17 M10 17 L13 14 M10 17 L13 20"/>
      </svg>;
    case 'tab-diffuse':
      // scattered dots radiating
      return <svg width={size} height={size} viewBox="0 0 24 24" fill={stroke}>
        <circle cx="12" cy="12" r="1.8"/>
        <circle cx="6"  cy="8"  r="1.2" opacity="0.8"/>
        <circle cx="18" cy="9"  r="1.2" opacity="0.8"/>
        <circle cx="7"  cy="16" r="1.2" opacity="0.6"/>
        <circle cx="17" cy="17" r="1.2" opacity="0.6"/>
        <circle cx="4"  cy="13" r="0.9" opacity="0.4"/>
        <circle cx="20" cy="14" r="0.9" opacity="0.4"/>
      </svg>;
    default: return null;
  }
};

// ─── Knob ──────────────────────────────────────────────────────────────────
const Knob = ({ value = 0.5, size = 44, label, sublabel, valueText, accent = 'var(--accent)', bipolar = false }) => {
  const r = size / 2;
  const rOuter = r - 2;
  const rInner = r - 8;
  const start = -135, end = 135;
  const angle = start + value * (end - start);
  const rad = (deg) => (deg * Math.PI) / 180;
  const arcPath = (a0, a1) => {
    const x0 = r + Math.cos(rad(a0 - 90)) * rOuter;
    const y0 = r + Math.sin(rad(a0 - 90)) * rOuter;
    const x1 = r + Math.cos(rad(a1 - 90)) * rOuter;
    const y1 = r + Math.sin(rad(a1 - 90)) * rOuter;
    const large = Math.abs(a1 - a0) > 180 ? 1 : 0;
    const sweep = a1 > a0 ? 1 : 0;
    return `M ${x0} ${y0} A ${rOuter} ${rOuter} 0 ${large} ${sweep} ${x1} ${y1}`;
  };
  const ind = {
    x1: r + Math.cos(rad(angle - 90)) * (rInner - 4),
    y1: r + Math.sin(rad(angle - 90)) * (rInner - 4),
    x2: r + Math.cos(rad(angle - 90)) * (rInner + 2),
    y2: r + Math.sin(rad(angle - 90)) * (rInner + 2),
  };

  return (
    <div className={'knob' + (size > 60 ? ' lg' : '')}>
      <div className="dial" style={{ width: size, height: size }}>
        <svg width={size} height={size}>
          <path d={arcPath(start, end)} stroke="var(--bg-elev)" strokeWidth="2" fill="none" strokeLinecap="round"/>
          {bipolar
            ? <path d={arcPath(0, angle)} stroke={accent} strokeWidth="2" fill="none" strokeLinecap="round"/>
            : <path d={arcPath(start, angle)} stroke={accent} strokeWidth="2" fill="none" strokeLinecap="round"/>}
          <defs>
            <radialGradient id={'kg-' + size} cx="50%" cy="40%" r="60%">
              <stop offset="0%" stopColor="#2a2a36"/>
              <stop offset="100%" stopColor="#15151c"/>
            </radialGradient>
          </defs>
          <circle cx={r} cy={r} r={rInner} fill={`url(#kg-${size})`} stroke="var(--line)" strokeWidth="0.8"/>
          <line x1={ind.x1} y1={ind.y1} x2={ind.x2} y2={ind.y2}
                stroke="#fff" strokeWidth="1.6" strokeLinecap="round"/>
        </svg>
      </div>
      {label && <div className="lbl">{label}</div>}
      {valueText && <div className="val mono">{valueText}</div>}
    </div>
  );
};

const Seg = ({ options, active, onChange, accent = false }) => (
  <div className={'seg' + (accent ? ' accent' : '')}>
    {options.map((o, i) => (
      <div key={i} className={'opt' + (i === active ? ' active' : '')}
           onClick={() => onChange && onChange(i)}>{o}</div>
    ))}
  </div>
);

const Toggle = ({ on, label, onChange }) => (
  <div className={'toggle' + (on ? ' on' : '')} onClick={() => onChange && onChange(!on)}>
    <span>{label}</span>
    <span className="switch"></span>
  </div>
);

// ─── HEADER ────────────────────────────────────────────────────────────────
const Header = ({ accent }) => (
  <div className="header">
    <div className="brand">
      <span className="brand-mark"><Icon name="logo" size={22} stroke={accent}/></span>
      <span className="brand-name">Petal</span>
      <span className="brand-sub">v2 · multitap</span>
    </div>
    <div className="preset-bar">
      <div className="preset">
        <span className="chev"><Icon name="chev-l"/></span>
        <span className="name">DEFAULT</span>
        <span className="chev"><Icon name="chev-r"/></span>
      </div>
    </div>
    <div className="header-right">
      <div className="icon-btn" title="Help"><Icon name="help"/></div>
      <div className="icon-btn" title="Undo"><Icon name="undo"/></div>
      <div className="icon-btn" title="Redo"><Icon name="redo"/></div>
      <div className="icon-btn" title="Randomize"><Icon name="dice"/></div>
      <div className="icon-btn" title="Save preset"><Icon name="save"/></div>
    </div>
  </div>
);

// ─── LEFT PANEL ────────────────────────────────────────────────────────────
const LeftPanel = ({ accent }) => {
  const [sync, setSync] = useState(1);
  const [mode, setMode] = useState(0);
  const [timeLinked, setTimeLinked] = useState(true);
  const [timeL, setTimeL] = useState(0.45);
  const [timeR, setTimeR] = useState(0.45);

  // formatter for display — when sync mode, show as ratio; when free, show ms
  const fmtTime = (v) => {
    if (sync === 1) {
      const ratios = ['1/32','1/16','1/8','1/4','3/8','1/2','3/4','1/1'];
      return ratios[Math.min(ratios.length-1, Math.floor(v * ratios.length))];
    }
    return (Math.round(2000 * v) + ' ms');
  };

  const onLChange = (v) => {
    setTimeL(v);
    if (timeLinked) setTimeR(v);
  };
  const onRChange = (v) => {
    setTimeR(v);
    if (timeLinked) setTimeL(v);
  };

  return (
    <div className="col-left">
      <div className="left-hero">
        <div className="time-knob">
          <Knob value={timeL} size={56} accent={accent}/>
          <div className="v">{fmtTime(timeL)}</div>
          <div className="lbl">Time L</div>
        </div>
        <div className={'link-time' + (timeLinked ? '' : ' off')}
             onClick={() => setTimeLinked(!timeLinked)}
             title={timeLinked ? 'Unlink L/R' : 'Link L/R'}>
          <Icon name="link" size={10} stroke="currentColor"/>
        </div>
        <div className="time-knob">
          <Knob value={timeR} size={56} accent={accent}/>
          <div className="v">{fmtTime(timeR)}</div>
          <div className="lbl">Time R</div>
        </div>
      </div>

      <div className="section">
        <div className="section-title"><span>Sync</span></div>
        <Seg options={['Free', 'Sync']} active={sync} onChange={setSync} accent/>
      </div>

      <div className="section">
        <div className="section-title"><span>Channel Mode</span></div>
        <Seg options={['Stereo','M / S','Mono']} active={mode} onChange={setMode}/>
      </div>

      <div className="section">
        <div className="section-title"><span>Levels</span></div>
        <div className="row-3">
          <Knob value={0.6}  label="Input"   valueText="+0.0" accent={accent}/>
          <Knob value={0.5}  label="Dry/Wet" valueText="50%"  accent={accent}/>
          <Knob value={0.55} label="Output"  valueText="+0.0" accent={accent}/>
        </div>
      </div>

      <div className="section">
        <div className="section-title"><span>Character</span></div>
        <div className="row-2">
          <Knob value={0.30} label="Wobble"  valueText="0.07 Hz" accent={accent}/>
          <Knob value={0.20} label="Diffuse" valueText="20%"     accent={accent}/>
        </div>
        <div style={{ height: 6 }}/>
        <Toggle on={false} label="Soft Clip"/>
      </div>
    </div>
  );
};

// ─── CENTER: Visualizer chrome ─────────────────────────────────────────────
const VizTabs = ({ mode, setMode, count, accent }) => (
  <div className="viz-tabs">
    <div className={'tab' + (mode === 'stems' ? ' active' : '')} onClick={() => setMode('stems')}>
      <Icon name="stems" size={14} stroke={mode === 'stems' ? accent : 'currentColor'}/>
      <span>Stems</span>
    </div>
    <div className={'tab' + (mode === 'grid' ? ' active' : '')} onClick={() => setMode('grid')}>
      <Icon name="grid" size={14} stroke={mode === 'grid' ? accent : 'currentColor'}/>
      <span>Grid</span>
    </div>
    <div className="meta">
      <span><span className="dot" style={{ background: accent, boxShadow: `0 0 8px ${accent}` }}/> Stereo</span>
      <span>Taps <span className="v">{count}</span></span>
      <span>Length <span className="v">4 / 1</span></span>
    </div>
  </div>
);

// ─── STEM visualizer (read-only overview, Cluster-Delay style) ─────────────
const StemViz = ({ taps, accent, showGrid }) => {
  const W = 1000, H = 360;
  const padL = 36, padR = 24, padT = 30, padB = 36;
  const drawW = W - padL - padR;
  const drawH = H - padT - padB;
  const midY = padT + drawH / 2;
  const maxBar = drawH / 2 - 12;

  return (
    <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet"
         style={{ width: '100%', height: '100%' }}>
      <text x={padL - 8} y={padT + 10} textAnchor="end" fontFamily="Geist Mono" fontSize="9"
            fill="var(--ink-faint)" letterSpacing="2">L</text>
      <text x={padL - 8} y={H - padB - 4} textAnchor="end" fontFamily="Geist Mono" fontSize="9"
            fill="var(--ink-faint)" letterSpacing="2">R</text>

      <line x1={padL} y1={midY} x2={W - padR} y2={midY}
            stroke="var(--line)" strokeWidth="0.8" strokeDasharray="2 4"/>

      {showGrid && Array.from({ length: 5 }).map((_, i) => {
        const x = padL + (i / 4) * drawW;
        return (
          <g key={i}>
            <line x1={x} y1={padT} x2={x} y2={H - padB}
                  stroke="var(--line)" strokeWidth={i === 0 || i === 4 ? 1 : 0.5}
                  strokeDasharray={i === 0 || i === 4 ? null : '1 4'}/>
            <text x={x} y={H - padB + 18} textAnchor="middle"
                  fontFamily="Geist Mono" fontSize="10" fill="var(--ink-faint)">{i}/1</text>
          </g>
        );
      })}
      {showGrid && Array.from({ length: 16 }).map((_, i) => {
        if (i % 4 === 0) return null;
        const x = padL + (i / 16) * drawW;
        return <line key={'sub'+i} x1={x} y1={padT} x2={x} y2={H - padB}
                     stroke="var(--line-soft)" strokeWidth="0.4"/>;
      })}

      {taps.map((tp, i) => {
        const x = padL + tp.time * drawW;
        const op = tp.on ? 1 : 0.22;
        // pan-aware split of gain into L/R
        const lWeight = Math.cos((tp.pan + 1) * Math.PI / 4);
        const rWeight = Math.sin((tp.pan + 1) * Math.PI / 4);
        const hL = tp.gain * lWeight * maxBar;
        const hR = tp.gain * rWeight * maxBar;
        return (
          <g key={i} opacity={op}>
            <line x1={x} y1={midY - 1} x2={x} y2={midY - hL}
                  stroke={accent} strokeWidth="2.2" strokeLinecap="round"/>
            <circle cx={x} cy={midY - hL} r="3" fill={accent}/>
            <circle cx={x} cy={midY - hL} r="5" fill={accent} opacity="0.25"/>
            <line x1={x} y1={midY + 1} x2={x} y2={midY + hR}
                  stroke={accent} strokeWidth="2.2" strokeLinecap="round" opacity="0.85"/>
            <circle cx={x} cy={midY + hR} r="3" fill={accent} opacity="0.85"/>
            <circle cx={x} cy={midY + hR} r="5" fill={accent} opacity="0.2"/>

            {tp.pitch !== 0 && tp.on && (
              <text x={x} y={midY - hL - 12} textAnchor="middle"
                    fontFamily="Geist Mono" fontSize="10" fill={accent} fontWeight="600">
                {tp.pitch > 0 ? '+' : ''}{tp.pitch}
              </text>
            )}
            <text x={x} y={H - padB + 6} textAnchor="middle"
                  fontFamily="Geist Mono" fontSize="8" fill="var(--ink-faint)" opacity="0.6">{i + 1}</text>
          </g>
        );
      })}
    </svg>
  );
};

// ─── GRID visualizer (editable, balls on time × volume) ────────────────────
const GridViz = ({ taps, setTaps, accent, showGrid }) => {
  const W = 1000, H = 360;
  const padL = 44, padR = 24, padT = 16, padB = 30;
  const drawW = W - padL - padR;
  const drawH = H - padT - padB;
  const svgRef = useRef(null);
  const [dragIdx, setDragIdx] = useState(null);

  const xt = (t) => padL + t * drawW;
  const yg = (g) => padT + (1 - g) * drawH; // gain 0 at bottom, 1 at top

  const fromSvg = (clientX, clientY) => {
    const svg = svgRef.current;
    if (!svg) return null;
    const pt = svg.createSVGPoint();
    pt.x = clientX; pt.y = clientY;
    const ctm = svg.getScreenCTM();
    if (!ctm) return null;
    const p = pt.matrixTransform(ctm.inverse());
    return p;
  };

  const onPointerDown = (i) => (e) => {
    e.preventDefault();
    setDragIdx(i);
  };

  const onPointerMove = (e) => {
    if (dragIdx === null) return;
    const p = fromSvg(e.clientX, e.clientY);
    if (!p) return;
    const t = Math.max(0, Math.min(1, (p.x - padL) / drawW));
    const g = Math.max(0, Math.min(1, 1 - (p.y - padT) / drawH));
    setTaps(prev => prev.map((tp, j) => j === dragIdx ? { ...tp, time: t, gain: g } : tp));
  };

  const onPointerUp = () => setDragIdx(null);

  useEffect(() => {
    if (dragIdx === null) return;
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    return () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
    };
  }, [dragIdx]);

  // pan-to-color: more negative pan = redder, more positive = bluer
  const tapColor = (pan) => {
    // hue shift around accent
    const hueShift = pan * 30; // ±30° hue
    return { fill: accent, opacity: 1, hueRotate: hueShift };
  };

  return (
    <svg ref={svgRef}
         viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet"
         style={{ width: '100%', height: '100%', touchAction: 'none' }}>

      {/* y-axis labels: 0 dB top, -inf bottom */}
      {[1, 0.75, 0.5, 0.25, 0].map(g => (
        <g key={g}>
          {showGrid && (
            <line x1={padL} y1={yg(g)} x2={padL + drawW} y2={yg(g)}
                  stroke="var(--line-soft)" strokeWidth="0.5"
                  strokeDasharray={g === 0 || g === 1 ? null : '2 4'}/>
          )}
          <text x={padL - 8} y={yg(g) + 3} textAnchor="end"
                fontFamily="Geist Mono" fontSize="9" fill="var(--ink-faint)">
            {g === 0 ? '-∞' : (g === 1 ? '0' : Math.round((g - 1) * 24)) }
          </text>
        </g>
      ))}
      <text x={padL - 8} y={padT - 4} textAnchor="end"
            fontFamily="Geist Mono" fontSize="8" fill="var(--ink-faint)" letterSpacing="2">dB</text>

      {/* x-axis beat verticals */}
      {showGrid && Array.from({ length: 5 }).map((_, i) => {
        const x = padL + (i / 4) * drawW;
        return (
          <g key={i}>
            <line x1={x} y1={padT} x2={x} y2={padT + drawH}
                  stroke="var(--line)" strokeWidth={i === 0 || i === 4 ? 1 : 0.6}
                  strokeDasharray={i === 0 || i === 4 ? null : '1 4'}/>
            <text x={x} y={H - 8} textAnchor="middle"
                  fontFamily="Geist Mono" fontSize="10" fill="var(--ink-faint)">{i}/1</text>
          </g>
        );
      })}
      {showGrid && Array.from({ length: 16 }).map((_, i) => {
        if (i % 4 === 0) return null;
        const x = padL + (i / 16) * drawW;
        return <line key={'sub'+i} x1={x} y1={padT} x2={x} y2={padT + drawH}
                     stroke="var(--line-soft)" strokeWidth="0.35"/>;
      })}

      {/* trailing line from t=0 baseline to each ball — like a "stem" */}
      {taps.map((tp, i) => {
        const x = xt(tp.time);
        const y = yg(tp.gain);
        const op = tp.on ? 0.45 : 0.1;
        return (
          <line key={'stem'+i}
                x1={x} y1={yg(0)} x2={x} y2={y}
                stroke={accent} strokeWidth="1" opacity={op}/>
        );
      })}

      {/* the balls */}
      {taps.map((tp, i) => {
        const x = xt(tp.time);
        const y = yg(tp.gain);
        const op = tp.on ? 1 : 0.25;
        // radius reflects pitch magnitude — pitched taps stand out
        const r = 8 + Math.abs(tp.pitch) * 0.4;
        const isDragging = dragIdx === i;
        return (
          <g key={i} opacity={op} style={{ cursor: 'grab' }}
             onPointerDown={onPointerDown(i)}>
            {/* outer glow ring */}
            <circle cx={x} cy={y} r={r + 6}
                    fill={accent} opacity={isDragging ? 0.3 : 0.12}/>
            {/* inner ball */}
            <circle cx={x} cy={y} r={r}
                    fill={accent}
                    stroke="#fff" strokeOpacity="0.25" strokeWidth="1"/>
            {/* pan indicator: small arc on either left or right of the ball */}
            {Math.abs(tp.pan) > 0.05 && (
              <path d={tp.pan < 0
                  ? `M ${x - r - 1} ${y - r * 0.6} A ${r + 3} ${r + 3} 0 0 0 ${x - r - 1} ${y + r * 0.6}`
                  : `M ${x + r + 1} ${y - r * 0.6} A ${r + 3} ${r + 3} 0 0 1 ${x + r + 1} ${y + r * 0.6}`}
                fill="none" stroke={accent} strokeWidth="1.5" opacity="0.8"/>
            )}
            {/* pitch label inside ball */}
            <text x={x} y={y + 3} textAnchor="middle"
                  fontFamily="Geist Mono" fontSize={tp.pitch === 0 ? 9 : 10}
                  fill="#fff" fontWeight={tp.pitch === 0 ? 400 : 600}
                  style={{ pointerEvents: 'none' }}>
              {tp.pitch === 0 ? (i + 1) : (tp.pitch > 0 ? '+' + tp.pitch : tp.pitch)}
            </text>
            {/* tap index outside */}
            {tp.pitch !== 0 && (
              <text x={x} y={y + r + 11} textAnchor="middle"
                    fontFamily="Geist Mono" fontSize="8" fill="var(--ink-faint)"
                    style={{ pointerEvents: 'none' }}>
                {i + 1}
              </text>
            )}
          </g>
        );
      })}

      {/* hint */}
      <text x={padL + 8} y={padT + 12}
            fontFamily="Geist" fontSize="9" fill="var(--ink-faint)"
            letterSpacing="0.12em">DRAG BALLS · TIME × GAIN</text>
    </svg>
  );
};

// ─── Compact shaping XY-pad strip ──────────────────────────────────────────
const ShapingStrip = ({ accent, linked, setLinked }) => {
  const renderPad = (label, dim) => (
    <div className="xy-pad" style={dim ? { opacity: 0.4, pointerEvents: 'none' } : {}}>
      <span className="ch-tag mono">{label}</span>
      <svg viewBox="0 0 240 120" preserveAspectRatio="none">
        {Array.from({ length: 3 }).map((_, i) => (
          <line key={'v'+i} x1={(i+1) * 60} y1="0" x2={(i+1) * 60} y2="120"
                stroke="var(--line)" strokeWidth="0.4" strokeDasharray="2 4"/>
        ))}
        {Array.from({ length: 3 }).map((_, i) => (
          <line key={'h'+i} x1="0" y1={(i+1) * 30} x2="240" y2={(i+1) * 30}
                stroke="var(--line)" strokeWidth="0.4" strokeDasharray="2 4"/>
        ))}
        <path d="M 10 100 Q 80 20 130 60 T 230 30"
              fill="none" stroke={accent} strokeWidth="1.4" opacity="0.85"/>
        <line x1="0" y1="60" x2="240" y2="60" stroke={accent} strokeWidth="0.6" opacity="0.4"/>
        <line x1="120" y1="0" x2="120" y2="120" stroke={accent} strokeWidth="0.6" opacity="0.4"/>
        <circle cx="120" cy="60" r="5" fill={accent}/>
        <circle cx="120" cy="60" r="10" fill={accent} opacity="0.25"/>
      </svg>
      <span className="corner-tag l">← tap 1</span>
      <span className="corner-tag r">tap 8 →</span>
    </div>
  );

  return (
    <div className="shaping">
      {renderPad('L', false)}
      {renderPad('R', linked)}
      <div className="shaping-right">
        <div className="section-title" style={{ fontSize: 8, marginBottom: 0 }}>SHAPE</div>
        <div className={'link-btn' + (linked ? '' : ' off')} onClick={() => setLinked(!linked)}>
          <Icon name="link" size={16} stroke="currentColor"/>
        </div>
        <div className="mono" style={{ fontSize: 9, color: 'var(--ink-faint)', letterSpacing: '0.15em', textTransform: 'uppercase' }}>
          {linked ? 'LINKED' : 'UNLINK'}
        </div>
      </div>
    </div>
  );
};

// ─── Center column ─────────────────────────────────────────────────────────
const CenterPanel = ({ vizMode, setVizMode, taps, setTaps, accent, showGrid, linked, setLinked }) => {
  const activeCount = taps.filter(t => t.on).length;
  return (
    <div className="col-center">
      <VizTabs mode={vizMode} setMode={setVizMode} count={activeCount} accent={accent}/>
      <div className="viz">
        {vizMode === 'stems'
          ? <StemViz taps={taps} accent={accent} showGrid={showGrid}/>
          : <GridViz taps={taps} setTaps={setTaps} accent={accent} showGrid={showGrid}/>}
      </div>
      <ShapingStrip accent={accent} linked={linked} setLinked={setLinked}/>
    </div>
  );
};

// ─── RIGHT PANEL ───────────────────────────────────────────────────────────
const TABS = [
  { id: 'pitch',   label: 'Pitch',   icon: 'tab-pitch' },
  { id: 'pan',     label: 'Pan',     icon: 'tab-pan' },
  { id: 'gain',    label: 'Gain',    icon: 'tab-gain' },
  { id: 'filter',  label: 'Filter',  icon: 'tab-filter' },
  { id: 'xfeed',   label: 'X-Feed',  icon: 'tab-xfeed' },
  { id: 'diffuse', label: 'Diffuse', icon: 'tab-diffuse' },
];

const TapRowPitch = ({ tap, i, onToggle, onPitch, accent }) => {
  const isPitched = tap.pitch !== 0;
  const sign = tap.pitch > 0 ? '+' : '';
  return (
    <div className={'tap-row' + (tap.on ? '' : ' off')}>
      <span className="num">{String(i+1).padStart(2,'0')}</span>
      <span className="dot" onClick={() => onToggle(i)}></span>
      <div className="stepper">
        <span className="b" onClick={() => onPitch(i, -1)}>−</span>
        <span className={'v' + (isPitched ? ' pitched' : '')} style={isPitched ? { color: accent } : {}}>
          {sign}{tap.pitch}
        </span>
        <span className="b" onClick={() => onPitch(i, +1)}>+</span>
      </div>
      <span className="slider-val">st</span>
    </div>
  );
};

const TapRowPan = ({ tap, i, accent }) => {
  const pct = ((tap.pan + 1) / 2) * 100;
  const label = tap.pan === 0 ? 'C' : (tap.pan > 0 ? `R${Math.round(tap.pan*100)}` : `L${Math.round(-tap.pan*100)}`);
  const leftFrac = tap.pan < 0 ? 50 + tap.pan * 50 : 50;
  const widthFrac = Math.abs(tap.pan) * 50;
  return (
    <div className={'tap-row' + (tap.on ? '' : ' off')}>
      <span className="num">{String(i+1).padStart(2,'0')}</span>
      <span className="dot"></span>
      <div className="mini-slider">
        <div className="fill" style={{ left: leftFrac + '%', width: widthFrac + '%', background: accent }}></div>
        <div className="thumb" style={{ left: pct + '%', borderColor: accent }}></div>
      </div>
      <span className="slider-val">{label}</span>
    </div>
  );
};

const TapRowGain = ({ tap, i, accent }) => {
  const v = tap.gain;
  return (
    <div className={'tap-row' + (tap.on ? '' : ' off')}>
      <span className="num">{String(i+1).padStart(2,'0')}</span>
      <span className="dot"></span>
      <div className="mini-slider">
        <div className="fill" style={{ left: 0, width: (v * 100) + '%', background: accent }}></div>
        <div className="thumb" style={{ left: (v * 100) + '%', borderColor: accent }}></div>
      </div>
      <span className="slider-val">{Math.round(-24 + v * 30)} dB</span>
    </div>
  );
};

const TapRowFilter = ({ tap, i, accent }) => (
  <div className={'tap-row' + (tap.on ? '' : ' off')}>
    <span className="num">{String(i+1).padStart(2,'0')}</span>
    <span className="dot"></span>
    <div className="mini-slider">
      <div className="fill" style={{ left: 0, width: (tap.lp * 100) + '%', background: accent }}></div>
      <div className="thumb" style={{ left: (tap.lp * 100) + '%', borderColor: accent }}></div>
    </div>
    <span className="slider-val">{Math.round(200 + tap.lp * 18000)} Hz</span>
  </div>
);

const PlainContent = ({ accent, label, desc }) => (
  <div style={{ padding: '6px 4px' }}>
    <div style={{ fontSize: 10, letterSpacing: '0.2em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 10 }}>{label}</div>
    <div style={{ fontSize: 11, color: 'var(--ink-dim)', lineHeight: 1.55, marginBottom: 14 }}>{desc}</div>
    <div className="row-2">
      <Knob value={0.30} label="Amount" valueText="30%" accent={accent}/>
      <Knob value={0.50} label="Spread" valueText="50%" accent={accent}/>
    </div>
    <div style={{ height: 14 }}/>
    <Seg options={['Bipolar','Neighbour','FDN']} active={0}/>
  </div>
);

const RightPanel = ({ taps, setTaps, accent }) => {
  const [tab, setTab] = useState(0);
  const onToggle = (i) => setTaps(prev => prev.map((t, j) => j === i ? { ...t, on: !t.on } : t));
  const onPitch  = (i, d) => setTaps(prev => prev.map((t, j) =>
      j === i ? { ...t, pitch: Math.max(-12, Math.min(12, t.pitch + d)) } : t));

  return (
    <div className="col-right">
      <div className="right-tabs">
        {TABS.map((t, i) => (
          <div key={i} className={'right-tab' + (i === tab ? ' active' : '')}
               onClick={() => setTab(i)}>
            <Icon name={t.icon} size={16}
                  stroke={i === tab ? accent : 'currentColor'}/>
            <span className="tip">{t.label}</span>
          </div>
        ))}
      </div>
      <div className="right-body">
        {tab === 0 && taps.map((tp, i) => <TapRowPitch  key={i} tap={tp} i={i} onToggle={onToggle} onPitch={onPitch} accent={accent}/>)}
        {tab === 1 && taps.map((tp, i) => <TapRowPan    key={i} tap={tp} i={i} accent={accent}/>)}
        {tab === 2 && taps.map((tp, i) => <TapRowGain   key={i} tap={tp} i={i} accent={accent}/>)}
        {tab === 3 && taps.map((tp, i) => <TapRowFilter key={i} tap={tp} i={i} accent={accent}/>)}
        {tab === 4 && <PlainContent accent={accent} label="Crossfeed"
                       desc="Route tap outputs into adjacent taps or the opposite stereo channel. Turns the delay network into a resonator."/>}
        {tab === 5 && <PlainContent accent={accent} label="Diffusion"
                       desc="Allpass network per tap — blurs transients into a smear of resonance."/>}
      </div>
      <div className="right-footer">
        <div className="fbk-block">
          <Knob value={0.5} size={40} accent={accent}/>
          <div className="stack">
            <span className="lbl">Feedback</span>
            <span className="val">50%</span>
          </div>
        </div>
        <div className="count-block">
          <div className="lbl">Taps</div>
          <div className="val" style={{ color: accent }}>{taps.length}</div>
          <div className="controls">
            <span className="b">−</span>
            <span className="b">+</span>
          </div>
        </div>
      </div>
    </div>
  );
};

// ─── App ───────────────────────────────────────────────────────────────────
function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [taps, setTaps] = useState(INITIAL_TAPS);
  const [linked, setLinked] = useState(true);
  const accent = t.accent;

  React.useEffect(() => {
    document.documentElement.style.setProperty('--accent', accent);
    document.documentElement.style.setProperty('--accent-soft', accent + '22');
  }, [accent]);

  return (
    <React.Fragment>
      <div className="plugin">
        <Header accent={accent}/>
        <div className="body">
          <LeftPanel accent={accent}/>
          <CenterPanel
            vizMode={t.vizMode}
            setVizMode={(m) => setTweak('vizMode', m)}
            taps={taps}
            setTaps={setTaps}
            accent={accent}
            showGrid={t.showGrid}
            linked={linked}
            setLinked={setLinked}
          />
          <RightPanel taps={taps} setTaps={setTaps} accent={accent}/>
        </div>
      </div>
    </React.Fragment>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<App />);
