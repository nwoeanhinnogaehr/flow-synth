module Main exposing (..)

import Html exposing (Html, button, div, text, ol, ul, li, b, i, br, code)
import Html.Events exposing (onClick)
import Html.Attributes exposing (..)
import Json.Decode as Decode
import Http
import Task


apiUrl =
    "http://localhost:8008/"



-- DATA --


type alias Model =
    { graph : Graph
    , err : Maybe String
    , mode : Mode
    }


emptyModel =
    { graph = emptyGraph
    , err = Nothing
    , mode = Normal
    }


type Mode
    = Normal
    | Connecting ConnectingState


type alias ConnectingState =
    { node : Node
    , port_ : Port
    , portType : PortType
    }


type alias Ports =
    { input : List Port
    , output : List Port
    }


type alias Port =
    { id : Int
    , edge : Maybe Edge
    , portType : PortType
    , name : String
    }


type PortType
    = Input
    | Output


type alias Edge =
    { nodeId : Int
    , portId : Int
    }


type alias Node =
    { id : Int
    , name : String
    , ports : Ports
    , status : NodeStatus
    , messageDescriptors : List MessageDescriptor
    }


type alias MessageDescriptor =
    { name : String
    , args : List MessageArg
    }


type alias MessageArg =
    { name : String
    , type_ : String
    }


type NodeStatus
    = Stopped
    | Running
    | Paused


type alias NodeType =
    { id : Int
    , name : String
    }


type alias Graph =
    { nodes : List Node
    , types : List NodeType
    }


type Msg
    = Refresh
    | RefreshNodes (Result Http.Error (List Node))
    | UpdateTypes (Result Http.Error (List NodeType))
    | AddNode NodeType
    | AddedNode (Result Http.Error AddNodeResult)
    | StartConnecting Node Port
    | DoConnect Node Port Node Port
    | Connected (Result Http.Error Decode.Value)
    | RunNode Node
    | PauseNode Node
    | RanNode (Result Http.Error Decode.Value)
    | PausedNode (Result Http.Error Decode.Value)
    | DoDisconnect Node Port Edge
    | Disconnected (Result Http.Error Decode.Value)


type alias AddNodeResult =
    Result String Int


emptyGraph =
    { nodes = []
    , types = []
    }



-- VIEW --


typesView : Model -> Html Msg
typesView model =
    div []
        [ text "Node types:"
        , ol [] (List.map (typeView model) model.graph.types)
        ]


typeView : Model -> NodeType -> Html Msg
typeView model node =
    li [] [ button [ onClick (AddNode node) ] [ text node.name ] ]


nodesView : Model -> Html Msg
nodesView model =
    div []
        [ text "Nodes:"
        , ol [] (List.map (nodeView model) model.graph.nodes)
        ]


nodeView : Model -> Node -> Html Msg
nodeView model node =
    li []
        [ div []
            [ b [] [ text node.name ]
            , nodeStatusView model node
            , ul [] (List.map (messageDescriptorView model node) node.messageDescriptors)
            , text "Inputs:"
            , ol [] (List.map (portView model node) node.ports.input)
            , text "Outputs:"
            , ol [] (List.map (portView model node) node.ports.output)
            ]
        ]


messageDescriptorView : Model -> Node -> MessageDescriptor -> Html Msg
messageDescriptorView model node desc =
    li []
        [ button [] [ text desc.name ]
        ]


nodeStatusView : Model -> Node -> Html Msg
nodeStatusView model node =
    div []
        ([ text ("(" ++ toString node.status ++ ") ") ]
            ++ if fullyConnected node then
                [ (let
                    ( label, action ) =
                        case node.status of
                            Stopped ->
                                ( "Run", RunNode node )

                            Paused ->
                                ( "Resume", RunNode node )

                            Running ->
                                ( "Pause", PauseNode node )
                   in
                    button [ onClick action ]
                        [ text label ]
                  )
                ]
               else
                []
        )


portView : Model -> Node -> Port -> Html Msg
portView model node port_ =
    -- what a fucking mess
    li []
        [ div []
            (text port_.name
                :: (case model.mode of
                        Normal ->
                            [ Maybe.withDefault
                                (div []
                                    [ text "Disconnected. "
                                    , button [ onClick (StartConnecting node port_) ] [ text "Connect this..." ]
                                    ]
                                )
                                (Maybe.map
                                    (\edge ->
                                        div []
                                            ((edgeView model edge)
                                                :: if node.status /= Running then
                                                    [ button [ onClick (DoDisconnect node port_ edge) ] [ text "Disconnect this" ] ]
                                                   else
                                                    []
                                            )
                                    )
                                    port_.edge
                                )
                            ]

                        Connecting state ->
                            if state.portType /= port_.portType then
                                [ (Maybe.withDefault
                                    (button
                                        [ onClick
                                            (if state.portType == Output then
                                                DoConnect state.node state.port_ node port_
                                             else
                                                DoConnect node port_ state.node state.port_
                                            )
                                        ]
                                        [ text "Connect here" ]
                                    )
                                    (Maybe.map (edgeView model) port_.edge)
                                  )
                                ]
                            else
                                [ (Maybe.withDefault
                                    (text "Disconnected.")
                                    (Maybe.map (edgeView model) port_.edge)
                                  )
                                ]
                   )
            )
        ]


edgeView : Model -> Edge -> Html Msg
edgeView model edge =
    b [] [ text (toString edge.nodeId ++ ":" ++ toString edge.portId) ]


errorView : Model -> Html Msg
errorView model =
    div [ style [ ( "color", "red" ) ] ] [ text (Maybe.withDefault "" model.err) ]



-- DECODE --


decodeNodes : Decode.Decoder (List Node)
decodeNodes =
    Decode.list
        (Decode.map5 Node
            (Decode.field "id" Decode.int)
            (Decode.field "name" Decode.string)
            (Decode.field "ports" decodePorts)
            (Decode.field "status" decodeNodeStatus)
            (Decode.field "message_descriptors" (Decode.list decodeMessageDescriptor))
        )


decodeMessageDescriptor : Decode.Decoder MessageDescriptor
decodeMessageDescriptor =
    Decode.map2 MessageDescriptor
        (Decode.field "name" Decode.string)
        (Decode.field "args" (Decode.list decodeMessageArg))


decodeMessageArg : Decode.Decoder MessageArg
decodeMessageArg =
    -- TODO decode types to enum
    Decode.map2 MessageArg
        (Decode.field "name" Decode.string)
        (Decode.field "type" Decode.string)


decodePorts : Decode.Decoder Ports
decodePorts =
    Decode.map2 Ports
        (Decode.field "in" (Decode.list (decodePort Input)))
        (Decode.field "out" (Decode.list (decodePort Output)))


decodePort : PortType -> Decode.Decoder Port
decodePort portType =
    Decode.map4 Port
        (Decode.field "id" Decode.int)
        (Decode.maybe
            (Decode.map2 Edge
                (Decode.at [ "edge", "node" ] Decode.int)
                (Decode.at [ "edge", "port" ] Decode.int)
            )
        )
        (Decode.succeed portType)
        (Decode.field "name" Decode.string)


decodeTypes : Decode.Decoder (List NodeType)
decodeTypes =
    Decode.list
        (Decode.map2 NodeType
            (Decode.field "id" Decode.int)
            (Decode.field "name" Decode.string)
        )


decodeAddNode : Decode.Decoder AddNodeResult
decodeAddNode =
    Decode.andThen
        (\status ->
            if status == "ok" then
                Decode.map Ok (Decode.field "id" Decode.int)
            else
                Decode.succeed (Err status)
        )
        (Decode.field "status" Decode.string)


decodeNodeStatus : Decode.Decoder NodeStatus
decodeNodeStatus =
    Decode.map
        (\msg ->
            case msg of
                "stopped" ->
                    Stopped

                "running" ->
                    Running

                "paused" ->
                    Paused

                _ ->
                    Stopped
        )
        Decode.string



-- ADD NODE --


addNode : NodeType -> Cmd Msg
addNode nodeType =
    let
        url =
            apiUrl ++ "type/" ++ toString nodeType.id ++ "/new"

        request =
            Http.get url decodeAddNode
    in
        Http.send AddedNode request



-- REFRESH --


refreshNodes : Cmd Msg
refreshNodes =
    let
        url =
            apiUrl ++ "node"

        request =
            Http.get url decodeNodes
    in
        Http.send RefreshNodes request


refreshTypes : Cmd Msg
refreshTypes =
    let
        url =
            apiUrl ++ "type"

        request =
            Http.get url decodeTypes
    in
        Http.send UpdateTypes request


refresh =
    Cmd.batch [ refreshTypes, refreshNodes ]



-- CONNECT --


doConnect : Node -> Port -> Node -> Port -> Cmd Msg
doConnect srcNode srcPort dstNode dstPort =
    let
        url =
            apiUrl
                ++ "node/connect/"
                ++ toString srcNode.id
                ++ "/"
                ++ toString srcPort.id
                ++ "/to/"
                ++ toString dstNode.id
                ++ "/"
                ++ toString dstPort.id

        request =
            Http.get url decodeConnect
    in
        Http.send Connected request


decodeConnect =
    Decode.value


fullyConnected node =
    List.all portIsConnected node.ports.input
        && List.all portIsConnected node.ports.output


portIsConnected port_ =
    port_.edge /= Nothing


doDisconnect : Node -> Port -> Edge -> Cmd Msg
doDisconnect node port_ edge =
    let
        ( endpointNode, endPointPort ) =
            case port_.portType of
                Input ->
                    ( node.id, port_.id )

                Output ->
                    ( edge.nodeId, edge.portId )

        url =
            apiUrl
                ++ "node/disconnect/"
                ++ toString endpointNode
                ++ "/"
                ++ toString endPointPort

        request =
            Http.get url decodeDisconnect
    in
        Http.send Disconnected request


decodeDisconnect =
    Decode.value



-- CONTROL STATUS --


runNode =
    setNodeStatus "run"


pauseNode =
    setNodeStatus "pause"


setNodeStatus : String -> Node -> Cmd Msg
setNodeStatus status node =
    let
        url =
            apiUrl ++ "node/set_status/" ++ toString node.id ++ "/" ++ status

        request =
            Http.get url decodeStatusUpdate
    in
        Http.send PausedNode request


decodeStatusUpdate =
    Decode.value



-- ERROR --


raiseError : Model -> String -> Model
raiseError model err =
    { model | err = Just err }



-- SPECIAL FUNCTIONS --


init : ( Model, Cmd Msg )
init =
    ( emptyModel
    , refresh
    )


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        Refresh ->
            ( model, Cmd.batch [ refreshNodes, refreshTypes ] )

        RefreshNodes (Ok newNodes) ->
            ( { model | graph = { types = model.graph.types, nodes = newNodes } }, Cmd.none )

        RefreshNodes (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        UpdateTypes (Ok newTypes) ->
            ( { model | graph = { types = newTypes, nodes = model.graph.nodes } }, Cmd.none )

        UpdateTypes (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        AddNode nodeType ->
            ( model, addNode nodeType )

        AddedNode (Ok nodeId) ->
            -- TODO show error here if needed (nodeId :: Result)
            ( model, refreshNodes )

        AddedNode (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        StartConnecting node port_ ->
            ( { model | mode = Connecting { node = node, port_ = port_, portType = port_.portType } }, Cmd.none )

        DoConnect srcNode srcPort dstNode dstPort ->
            ( { model | mode = Normal }, doConnect srcNode srcPort dstNode dstPort )

        Connected (Ok value) ->
            ( model, refreshNodes )

        Connected (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        RunNode node ->
            ( model, runNode node )

        RanNode (Ok value) ->
            ( model, refreshNodes )

        RanNode (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        PauseNode node ->
            ( model, pauseNode node )

        PausedNode (Ok value) ->
            ( model, refreshNodes )

        PausedNode (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        DoDisconnect node port_ edge ->
            ( model, doDisconnect node port_ edge )

        Disconnected (Ok value) ->
            ( model, refreshNodes )

        Disconnected (Err err) ->
            ( raiseError model (toString err), Cmd.none )


subscriptions : Model -> Sub Msg
subscriptions model =
    Sub.none


view : Model -> Html Msg
view model =
    div []
        [ errorView model
        , typesView model
        , nodesView model
        ]


main =
    Html.program { init = init, view = view, update = update, subscriptions = subscriptions }
