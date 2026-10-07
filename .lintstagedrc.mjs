export default {
  'server-rust/**/*.rs': 'rustfmt --edition 2021',
  'gui-gpui/**/*.rs': 'rustfmt --edition 2024',
  'gui/src-tauri/**/*.rs': 'rustfmt --edition 2021',
  'gui/src/**/*.{js,jsx,ts,tsx,json}': [
    'pnpm --dir gui exec prettier --write',
    'pnpm --dir gui exec eslint --fix',
  ],
};
