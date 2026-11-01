Feature: Deliver item events to Lua handlers
  "barrs item trigger" sends an event to the handler of one item, as a click
  or a hover on the bar does. The handlers share one Lua state until the next
  reload.

  Scenario: A click reaches the click handler of the item
    Given a running daemon with a clock item that records its clicks
    When I run "barrs item trigger clock click"
    Then the command succeeds
    And the clock handler recorded "clock:click"

  Scenario: An event for an unknown item is rejected
    Given a running daemon with a clock item that records its clicks
    When I run "barrs item trigger missing hover-enter"
    Then the command fails with "unknown item missing"
    And the clock handler recorded nothing
    And the daemon answers a ping

  Scenario: Handler state persists between events
    Given a running daemon with a clock item that counts its clicks
    When I run "barrs item trigger clock click"
    And I run "barrs item trigger clock click"
    Then the clock handler recorded "2"

  Scenario: A reload starts a new Lua state
    Given a running daemon with a clock item that counts its clicks
    When I run "barrs item trigger clock click"
    And I run "barrs item trigger clock click"
    And I run "barrs reload"
    And I run "barrs item trigger clock click"
    Then the clock handler recorded "1"
