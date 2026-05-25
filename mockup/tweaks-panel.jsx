// Minimal stub for the Claude Design Tweaks panel host bridge.
// In production we don't need the live-editing UI — we just provide defaults.

window.useTweaks = function(defaults) {
  const [state, setState] = React.useState(defaults);
  const set = (key, value) => setState(prev => ({ ...prev, [key]: value }));
  return [state, set];
};

window.TweaksPanel    = () => null;
window.TweakSection   = () => null;
window.TweakToggle    = () => null;
window.TweakRadio     = () => null;
window.TweakColor     = () => null;
window.TweakSlider    = () => null;
