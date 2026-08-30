export type Note = {
  id: string;
  mediaPath: string;
  positionMs: number;
  body: string;
  createdAt: string;
  updatedAt: string;
};

export type NoteCreate = {
  mediaPath: string;
  positionMs: number;
  body: string;
};

export type NoteUpdate = {
  id: string;
  body?: string;
  positionMs?: number;
};
