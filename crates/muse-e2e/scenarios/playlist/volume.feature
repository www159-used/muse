Feature: Application volume

  Scenario: Set, step, and clamp the internal volume
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then the playback volume is "100%"
    And the playback gain is "1.0000"

    When I set the playback volume to 50
    Then the playback volume is "50%"
    And the playback gain is "0.2500"
    And the volume command reports the same volume and gain

    When I increase the playback volume by 5
    Then the playback volume is "55%"

    When I decrease the playback volume by 5
    Then the playback volume is "50%"

    When I increase the playback volume by 1000
    Then the playback volume is "100%"

    When I decrease the playback volume by 1000
    Then the playback volume is "0%"
    And the playback gain is "0.0000"

    When I set the playback volume to 50
    And I advance to the next track
    Then the playback volume is "50%"
    And the playback gain is "0.2500"

  Scenario: Invalid volume commands explain the legal form
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then setting the playback volume to 999 fails
    And increasing the playback volume by "abc" fails
    And the playback volume is "100%"
    And the playback gain is "1.0000"

  Scenario: Volume persists across playback service restart and is unavailable when it is down
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"

    When I import the folder "downloads"
    And I play the playlist "全部本地"
    And I set the playback volume to 50

    Then the playback volume is "50%"
    And the playback gain is "0.2500"
    And the status names a volume file

    When I restart the playback service
    Then the playback volume is "50%"
    And the playback gain is "0.2500"
    And the volume command reports the same volume and gain

    When I shut down the playback service
    Then the volume command fails because playback is not running
