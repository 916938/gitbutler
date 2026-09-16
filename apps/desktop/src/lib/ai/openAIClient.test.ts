import { OpenAIClient } from "$lib/ai/openAIClient";
import { DeepSeekModelName, MessageRole, OpenAIModelName } from "$lib/ai/types";
import { describe, expect, test, vi, beforeEach } from "vitest";

// Mock the OpenAI SDK so tests don't make real network calls.
// The mock create function is exported as a named export for test access.
const mockCreate = vi.fn().mockResolvedValue([]);
vi.mock("openai", () => ({
	default: vi.fn().mockImplementation(() => ({
		chat: {
			completions: {
				create: mockCreate,
			},
		},
	})),
}));

import OpenAI from "openai";

describe("OpenAIClient", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockCreate.mockResolvedValue([]);
	});

	describe("constructor with DeepSeek model", () => {
		test("When constructed with DeepSeekModelName.Flash, it does not throw", () => {
			expect(
				() => new OpenAIClient("test-key", DeepSeekModelName.Flash, "https://api.deepseek.com"),
			).not.toThrow();
		});

		test("When constructed with DeepSeekModelName.Pro, it does not throw", () => {
			expect(
				() => new OpenAIClient("test-key", DeepSeekModelName.Pro, "https://api.deepseek.com"),
			).not.toThrow();
		});

		test("When constructed with a DeepSeek model, it passes the baseURL to the OpenAI SDK", () => {
			new OpenAIClient("dk-key", DeepSeekModelName.Pro, "https://api.deepseek.com");

			expect(OpenAI).toHaveBeenCalledWith(
				expect.objectContaining({
					apiKey: "dk-key",
					baseURL: "https://api.deepseek.com",
					dangerouslyAllowBrowser: true,
				}),
			);
		});
	});

	describe("constructor with OpenAI model", () => {
		test("When constructed with OpenAIModelName, it does not throw", () => {
			expect(() => new OpenAIClient("oai-key", OpenAIModelName.GPT54Nano, undefined)).not.toThrow();
		});

		test("When constructed without a baseURL, it passes undefined to the OpenAI SDK", () => {
			new OpenAIClient("oai-key", OpenAIModelName.GPT54Nano, undefined);

			expect(OpenAI).toHaveBeenCalledWith(
				expect.objectContaining({
					apiKey: "oai-key",
					baseURL: undefined,
					dangerouslyAllowBrowser: true,
				}),
			);
		});
	});

	describe("constructor with OpenRouter model", () => {
		test("When constructed with an OpenRouter-style model name, it does not throw", () => {
			expect(() => new OpenAIClient("or-key", "openai/gpt-4.1-mini", undefined)).not.toThrow();
		});
	});

	describe("#evaluate", () => {
		test("When evaluate is called with a DeepSeek model, it sends the correct model name to the API", async () => {
			const client = new OpenAIClient("dk-key", DeepSeekModelName.Pro, "https://api.deepseek.com");

			await client.evaluate([{ role: MessageRole.User, content: "hello" }]);

			expect(mockCreate).toHaveBeenCalledWith(
				expect.objectContaining({
					model: DeepSeekModelName.Pro,
				}),
			);
		});

		test("When evaluate is called with an OpenAI model, it sends the correct model name to the API", async () => {
			const client = new OpenAIClient("oai-key", OpenAIModelName.GPT54, undefined);

			await client.evaluate([{ role: MessageRole.User, content: "hello" }]);

			expect(mockCreate).toHaveBeenCalledWith(
				expect.objectContaining({
					model: OpenAIModelName.GPT54,
				}),
			);
		});
	});
});
