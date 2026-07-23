function record_time_event(ctx)
	-- Keep a deterministic in-process record of the latest event.
	last_time_event = ctx.event
end

return {
	bar = {
		spacing = 0,
		background = "#000000",
		notch_width = 200,
		notch_padding = 8,
		notch_offset = 0,
		notch_display_height = 0,
	},
	items = {
		{
			id = "workspaces",
			icon = "󰆍",
			placement = "left",
			plugin = { kind = "rift_workspaces" },
			hover = { tooltip = "Current Rift workspaces" },
		},
		{
			id = "cpu",
			icon = "󰘚",
			placement = "right",
			interval = 2,
			plugin = { kind = "cpu" },
			hover = { tooltip = "CPU usage" },
		},
		{
			id = "gpu",
			icon = "󰢮",
			placement = "right",
			interval = 2,
			plugin = { kind = "gpu" },
			hover = { tooltip = "GPU usage" },
		},
		{
			id = "battery",
			icon = "󰂄",
			placement = "right",
			interval = 10,
			plugin = { kind = "battery" },
			hover = { tooltip = "Battery status" },
		},
		{
			id = "time",
			icon = "󰥔",
			placement = "middle",
			interval = 1,
			plugin = { kind = "time" },
			handlers = { click = "record_time_event" },
			hover = { tooltip = "Current time" },
		},
		{
			id = "date",
			icon = "󰃭",
			placement = "middle",
			interval = 1800,
			plugin = { kind = "date", format = "%Y-%m-%d" },
			hover = { tooltip = "Current date" },
		},
	},
}
