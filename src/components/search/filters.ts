export interface ResultFilters {
  platform: string;
  cond: "any" | "new" | "used";
  min: string;
  max: string;
  freeShip: boolean;
  goodDeal: boolean;
  /** v26.03 风险：不限 / 不看高风险 / 只看低风险 */
  risk: "any" | "no_high" | "low";
  /** v26.03 店铺类型（ShopKind），空 = 不限 */
  shops: string[];
}

export const blankFilters = (): ResultFilters => ({
  platform: "all", cond: "any", min: "", max: "", freeShip: false, goodDeal: false, risk: "any", shops: [],
});
