Feature: Playlist repeat modes

  Scenario: Cycling mode walks sequential, list loop, and single loop
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then the playback mode is "sequential"

    When I cycle the playback mode
    Then the playback mode is "list loop"

    When I cycle the playback mode
    Then the playback mode is "single loop"

    When I cycle the playback mode
    Then the playback mode is "sequential"
