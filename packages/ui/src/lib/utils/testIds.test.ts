import { describe, expect, test } from "vitest";
import { TestId } from "./testIds";

describe("TestId", () => {
	describe("upstream-merged members", () => {
		test("BranchHeaderContextMenu_CreatePR exists with correct value", () => {
			expect(TestId.BranchHeaderContextMenu_CreatePR).toBeDefined();
			expect(TestId.BranchHeaderContextMenu_CreatePR).toBe("branch-header-context-menu-create-pr");
		});

		test("BranchHeaderContextMenu_Land exists with correct value", () => {
			expect(TestId.BranchHeaderContextMenu_Land).toBeDefined();
			expect(TestId.BranchHeaderContextMenu_Land).toBe("branch-header-context-menu-land");
		});
	});

	describe("enum integrity", () => {
		test("All TestId values are unique strings", () => {
			const values = Object.values(TestId);
			const uniqueValues = new Set(values);

			expect(values.length).toBe(uniqueValues.size);
			values.forEach((v) => {
				expect(typeof v).toBe("string");
			});
		});

		test("All TestId values are non-empty strings", () => {
			Object.values(TestId).forEach((v) => {
				expect(v.length).toBeGreaterThan(0);
			});
		});
	});
});
