import { Job } from "./job.d.ts";

let a: number;
const job: Job<[number, number]> = {
	params: [
		{
			name: "x",
			type: "number",
		},
		{
			name: "y",
			type: "number",
		},
	],
	stages: {
		stage1: (x: number, y: number) => {
			a = x + y;
		},
		stage2: () => {
			a += 2;
		},
		stage0: () => {
			console.log("yes");
		},
		final_stage: () => {
			return a;
		},
	},
};
export default job;
