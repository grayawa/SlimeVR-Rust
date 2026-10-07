import { desktopHost } from '@/platform';
export async function openUrl(url: string) {
  if (desktopHost?.api) {
    desktopHost?.api.openUrl(url);
  } else {
    window.open(url, '_blank');
  }
}
