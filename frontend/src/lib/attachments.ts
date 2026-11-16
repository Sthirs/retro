/**
 * Image attachments of reflections and tasks (spec §13, ADR-0013). The server
 * converts every upload to WebP; rich text references it by id only.
 */
import { ApiError, api, apiError } from '@/api/client';

/** What the server accepts (spec §13); it checks the bytes, not this type. */
export const ACCEPTED_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
export const MAX_ATTACHMENT_BYTES = 5 * 1024 * 1024;

export interface UploadedImage {
  id: string;
  width: number;
  height: number;
}

/** Where an attachment is served; fetch it with `loadImage` (it needs the token). */
export function attachmentUrl(id: string): string {
  return `/api/v1/attachments/${id}`;
}

/** True for a file the server may accept, judged by its declared type. */
export function isImageFile(file: File): boolean {
  return ACCEPTED_TYPES.includes(file.type);
}

/**
 * Uploads `file` for `teamId`. Rejects with an `ApiError` whose code is the
 * server's (`image_too_large`, `unsupported_image_type`, `invalid_image`, …),
 * checking type and size first to spare the upload.
 */
export async function uploadAttachment(teamId: string, file: File): Promise<UploadedImage> {
  if (!isImageFile(file)) throw new ApiError(415, 'unsupported_image_type');
  if (file.size > MAX_ATTACHMENT_BYTES) throw new ApiError(413, 'image_too_large');
  const res = await api.uploadAttachment(teamId, file, {
    headers: { 'Content-Type': file.type },
  });
  if (res.status !== 201) {
    // A body far over the limit can be refused without a problem document.
    throw res.status === 413 ? new ApiError(413, 'image_too_large') : apiError(res);
  }
  return { id: res.data.id, width: res.data.width, height: res.data.height };
}
