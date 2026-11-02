export interface ResultFilters {
  platform: string;
  cond: "any" | "new" | "used";
  min: string;
  max: string;
  freeShip: boolean;
  goodDeal: boolean;
}

export const blankFilters = (): ResultFilters => ({ platform: "all", cond: "any", min: "", max: "", freeShip: false, goodDeal: false });
