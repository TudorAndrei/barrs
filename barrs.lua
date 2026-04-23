function open_clock(ctx)
	return true
end

return {
	bar = {
		spacing = 0,
		background = "#000000",
	},
	items = {
		{
			id = "workspaces",
			icon = "󰆍",
			plugin = { kind = "rift_workspaces" },
			hover = { tooltip = "Current Rift workspaces" },
		},
		{
			id = "layout",
			icon = "󰙀",
			plugin = { kind = "rift_layout" },
			hover = { tooltip = "Current Rift layout" },
		},
		{
			id = "cpu",
			icon = "󰘚",
			plugin = { kind = "cpu" },
			hover = { tooltip = "CPU usage" },
		},
		{
			id = "gpu",
			icon = "󰢮",
			plugin = { kind = "gpu" },
			hover = { tooltip = "GPU usage" },
		},
		{
			id = "battery",
			icon = "󰂄",
			plugin = { kind = "battery" },
			hover = { tooltip = "Battery status" },
		},
		{
			id = "time",
			icon = "󰥔",
			plugin = { kind = "time" },
			handlers = { click = "open_clock" },
			hover = { tooltip = "Current time" },
		},
	},
}
