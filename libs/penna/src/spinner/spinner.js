{
  const motion = new URLSearchParams(location.search).get("motion");
  if (motion === "0" || motion === "off") document.documentElement.dataset.motion = "off";
}
